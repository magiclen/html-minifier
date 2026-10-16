use std::str::from_utf8;

use educe::Educe;

use crate::{
    HTMLMinifierError, HTMLWriter, css_minifier::CSSMinifier, foreign_content::ForeignContent,
    functions::*, js_minifier::JSMinifier, script_data::ScriptData,
};

/// The longest `type` attribute value that is collected.
/// Longer values are kept without minification.
const MAX_TYPE_LENGTH: usize = 256;

/// The longest tag or attribute name that is collected.
/// A longer name never matches a name that is checked.
const MAX_NAME_LENGTH: usize = 16;

/// Check whether the `type` attribute value of a `script` or `style` element enables minification.
/// Return `Some(true)` for a JavaScript module.
fn embedded_type(tag: &[u8], value: &[u8]) -> Option<bool> {
    // Every supported type is ASCII, and decoding entities never removes a non-ASCII byte.
    if value.len() > MAX_TYPE_LENGTH || !value.is_ascii() {
        return None;
    }

    let decoded;
    let value = if value.contains(&b'&') {
        decoded = html_escape::decode_html_entities(from_utf8(value).unwrap_or_default());

        decoded.as_bytes()
    } else {
        value
    };

    if tag == b"script" {
        if value.eq_ignore_ascii_case(b"module") {
            Some(true)
        } else {
            (value.is_empty()
                || [b"application/javascript".as_slice(), b"text/javascript"]
                    .iter()
                    .any(|t| value.trim_ascii().eq_ignore_ascii_case(t)))
            .then_some(false)
        }
    } else {
        (value.is_empty() || value.eq_ignore_ascii_case(b"text/css")).then_some(false)
    }
}

#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum Step {
    #[educe(Default)]
    Initial,
    InitialRemainOneWhitespace,
    InitialIgnoreWhitespace,
    StartTagInitial,
    EndTagInitial,
    StartTag,
    StartTagIn,
    StartTagAttributeName,
    StartTagAttributeNameWaitingValue,
    StartTagAttributeValueInitial,
    StartTagUnquotedAttributeValue,
    StartTagQuotedAttributeValue,
    EndTag,
    TagEnd,
    Doctype,
    Comment,
    ScriptDefault,
    ScriptJavaScript,
    StyleDefault,
    StyleCSS,
    Pre,
    Code,
    Textarea,
    RawText,
    Foreign,
    Plaintext,
}

impl Step {
    #[inline]
    const fn end_tag(self, raw_end: &'static [u8]) -> Option<&'static [u8]> {
        match self {
            Step::ScriptDefault | Step::ScriptJavaScript => Some(b"</script"),
            Step::StyleDefault | Step::StyleCSS => Some(b"</style"),
            Step::Textarea => Some(b"</textarea"),
            Step::RawText => Some(raw_end),
            _ => None,
        }
    }

    #[inline]
    const fn is_embedded(self) -> bool {
        matches!(self, Step::ScriptJavaScript | Step::StyleCSS)
    }

    /// Check whether the step is in raw text or preserved content, such as `<script>` and `<pre>`.
    #[inline]
    const fn is_special(self) -> bool {
        matches!(
            self,
            Step::ScriptDefault
                | Step::ScriptJavaScript
                | Step::StyleDefault
                | Step::StyleCSS
                | Step::Pre
                | Step::Code
                | Step::Textarea
                | Step::RawText
                | Step::Foreign
                | Step::Plaintext
        )
    }
}

/// Check whether a byte of text content is written as it is.
#[inline]
const fn is_plain_text(e: u8) -> bool {
    !(is_whitespace(e) || matches!(e, b'<' | b'&') || is_ascii_control(e))
}

/// Collect a byte of a tag or attribute name, up to one byte over the limit.
#[inline]
fn push_name_byte(name: &mut Vec<u8>, e: u8) {
    if name.len() <= MAX_NAME_LENGTH {
        name.push(e);
    }
}

#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum PreservedStep {
    #[educe(Default)]
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    QuotedAttributeValue,
    UnquotedAttributeValue,
    Declaration,
    BogusComment,
    Comment,
    Raw,
}

/// This struct helps you generate and minify your HTML code in the same time. The output destination is outside this struct.
#[derive(Educe, Clone)]
#[educe(Debug, Default(new))]
pub struct HTMLMinifierHelper {
    #[educe(Default = true)]
    /// Remove HTML comments, keeping empty comments where text could form a character reference or a tag.
    pub remove_comments: bool,
    #[educe(Default = true)]
    /// Minify the content in the `code` element.
    pub minify_code:     bool,
    #[educe(Default = true)]
    /// Minify JavaScript in supported script elements.
    pub minify_js:       bool,
    #[educe(Default = true)]
    /// Minify CSS in supported style elements.
    pub minify_css:      bool,

    // Buffers
    #[educe(Debug(method = "str_bytes_fmt"))]
    buffer:         Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    tag:            Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    attribute_type: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    preserved_tag:  Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    held:           [u8; 8],

    // Embedded code
    css:         CSSMinifier,
    js:          JSMinifier,
    script_data: ScriptData,
    foreign:     ForeignContent,

    // Steps
    step:                Step,
    step_counter:        u8,
    /// The name of the preserved element, such as `pre`, whose end tag ends the preserved content.
    #[educe(Debug(method = "str_bytes_fmt"))]
    preserved_root:      &'static [u8],
    preserved_step:      PreservedStep,
    preserved_depth:     usize,
    preserved_counter:   u8,
    preserved_end_tag:   bool,
    preserved_plaintext: bool,
    #[educe(Debug(method = "str_bytes_fmt"))]
    raw_end:             &'static [u8],
    preserved_quote:     u8,

    // Temp
    quote:      u8,
    last_space: u8,
    last_cr:    bool,

    // Flags
    /// Whether the text ends with an unfinished character reference or a literal `<`, which the text after a removed comment or `</>` could join.
    reference_pending:    bool,
    /// Whether the whitespace before removed comments has already been written.
    text_space_written:   bool,
    /// Whether the current tag or comment is at the start of the document.
    at_start:             bool,
    quoted_value_spacing: bool,
    quoted_value_empty:   bool,
    in_handled_attribute: bool,
    in_attribute_type:    bool,
    type_seen:            bool,
    type_supported:       bool,
    type_module:          bool,
    in_end_tag:           bool,
}

impl HTMLMinifierHelper {
    #[inline]
    fn skip_plain_text(&mut self, text_bytes: &[u8], mut p: usize) -> usize {
        let e = text_bytes[p];

        self.reference_pending =
            e == b'&' || (self.reference_pending && (e.is_ascii_alphanumeric() || e == b'#'));
        self.text_space_written = false;

        while let Some(&next) = text_bytes.get(p + 1) {
            let skip = if self.reference_pending {
                next.is_ascii_alphanumeric() || next == b'#'
            } else {
                is_plain_text(next)
            };

            if !skip {
                break;
            }

            p += 1;
        }

        p
    }

    /// Reset the state for a new tag.
    #[inline]
    fn begin_tag(&mut self) {
        self.in_end_tag = false;
        self.at_start = false;
        self.attribute_type.clear();
        self.type_seen = false;
        self.type_supported = true;
        self.type_module = false;
    }

    #[inline]
    fn set_flags_by_attribute(&mut self) {
        let is_type = self.buffer == b"type" && matches!(self.tag.as_slice(), b"script" | b"style");

        self.in_handled_attribute = self.buffer == b"class";

        // Like browsers, only the first `type` attribute counts.
        // Its value is collected only when it can enable minification.
        self.in_attribute_type = is_type
            && !self.type_seen
            && if self.tag == b"script" { self.minify_js } else { self.minify_css };

        if is_type {
            self.type_seen = true;
        }
    }

    /// Collect a byte of the `type` attribute value, up to one byte over the limit.
    #[inline]
    fn push_type_byte(&mut self, e: u8) {
        if self.in_attribute_type && self.attribute_type.len() <= MAX_TYPE_LENGTH {
            self.attribute_type.push(e);
        }
    }

    #[inline]
    fn finish_buffer(&mut self) {
        if self.in_attribute_type {
            let embedded_type = embedded_type(&self.tag, &self.attribute_type);

            self.type_supported = embedded_type.is_some();
            self.type_module = embedded_type == Some(true);
        }
    }

    #[inline]
    fn end_start_tag_and_get_next_step(
        &mut self,
        out: &mut impl HTMLWriter,
        text_bytes: &[u8],
        start: &mut usize,
        p: usize,
    ) -> Result<Step, HTMLMinifierError> {
        let step = match self.tag.as_slice() {
            b"script" => {
                self.step_counter = 0;
                self.script_data.reset();

                if self.minify_js && self.type_supported {
                    out.push_bytes(&text_bytes[*start..=p])?;
                    *start = p + 1;

                    self.js.reset(self.type_module);

                    Step::ScriptJavaScript
                } else {
                    Step::ScriptDefault
                }
            },
            b"style" => {
                self.step_counter = 0;

                if self.minify_css && self.type_supported {
                    out.push_bytes(&text_bytes[*start..=p])?;
                    *start = p + 1;

                    self.css.reset();

                    Step::StyleCSS
                } else {
                    Step::StyleDefault
                }
            },
            b"pre" => {
                self.start_preserved(b"pre");
                Step::Pre
            },
            b"listing" => {
                // Browsers keep the whitespace of `listing` like `pre`.
                self.start_preserved(b"listing");
                Step::Pre
            },
            b"code" => {
                if self.minify_code {
                    self.last_space = 0;

                    Step::InitialRemainOneWhitespace
                } else {
                    self.start_preserved(b"code");
                    Step::Code
                }
            },
            b"textarea" => {
                self.step_counter = 0;
                Step::Textarea
            },
            b"plaintext" => Step::Plaintext,
            _ => {
                self.last_space = 0;

                if let Some(end_tag) = raw_text_end_tag(&self.tag) {
                    self.step_counter = 0;
                    self.raw_end = end_tag;
                    Step::RawText
                } else {
                    Step::InitialRemainOneWhitespace
                }
            },
        };

        Ok(step)
    }

    #[inline]
    fn write_span(
        &mut self,
        bytes: &[u8],
        out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        if self.step.is_embedded() { self.feed_embedded(bytes, out) } else { out.push_bytes(bytes) }
    }

    #[inline]
    fn feed_embedded(
        &mut self,
        bytes: &[u8],
        out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        if self.step == Step::ScriptJavaScript {
            self.js.feed(bytes, out)
        } else {
            self.css.feed(bytes, out)
        }
    }

    #[inline]
    fn finish_embedded(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        if self.step == Step::ScriptJavaScript { self.js.finish(out) } else { self.css.finish(out) }
    }

    /// Check whether the step is in a `title` element, whose whitespace is collapsed.
    #[inline]
    fn in_title(&self) -> bool {
        self.step == Step::RawText && self.raw_end == b"</title"
    }

    /// Skip the bytes of raw text or preserved content that can neither start an end tag nor need a change.
    #[inline]
    fn skip_content(&self, text_bytes: &[u8], mut p: usize) -> usize {
        // A LF after a CR is removed, so it is not skipped.
        if self.follows_cr(text_bytes, p) {
            return p;
        }

        if self.step == Step::Foreign
            || (matches!(self.step, Step::Pre | Step::Code) && self.foreign.is_active())
        {
            return self.foreign.skip(text_bytes, p);
        }

        let script = matches!(self.step, Step::ScriptDefault | Step::ScriptJavaScript);
        let plaintext = self.step == Step::Plaintext;
        let title = self.in_title();
        let plain = if plaintext {
            true
        } else if matches!(self.step, Step::Pre | Step::Code) {
            self.preserved_step == PreservedStep::Data
        } else {
            self.step_counter == 0 && (!script || self.script_data.can_skip())
        };

        if plain {
            while let Some(&e) = text_bytes.get(p) {
                if (!plaintext && e == b'<')
                    || e == b'\r'
                    || is_ascii_control(e)
                    || (script && e == b'-' && self.script_data.tracks_dash())
                    || (title && is_whitespace(e))
                {
                    break;
                }

                p += 1;
            }
        }

        p
    }

    /// Skip the rest of a quoted attribute value that needs no change after the byte at `p`.
    #[inline]
    fn skip_quoted_value(&self, text_bytes: &[u8], mut p: usize) -> usize {
        // A `type` value is collected, and a LF after a CR is removed.
        if self.in_attribute_type || text_bytes[p] == b'\r' {
            return p;
        }

        while let Some(&e) = text_bytes.get(p + 1) {
            if e == self.quote
                || e == b'\r'
                || is_ascii_control(e)
                || (self.in_handled_attribute && is_whitespace(e))
            {
                break;
            }

            p += 1;
        }

        p
    }

    /// Leave raw text or preserved content after the name of its end tag.
    #[inline]
    fn end_raw_text(&mut self, e: u8) {
        self.last_space = 0;

        if e == b'>' {
            self.step = Step::InitialRemainOneWhitespace;
        } else {
            self.in_end_tag = true;
            self.begin_end_tag_attributes();
        }
    }

    /// Start removing the attributes of an end tag.
    /// The attribute steps of `PreservedStep` follow them, so that a `>` in a quoted value does not end the tag.
    #[inline]
    fn begin_end_tag_attributes(&mut self) {
        self.preserved_step = PreservedStep::BeforeAttributeName;
        self.step = Step::TagEnd;
    }

    /// Process a byte of embedded JavaScript or CSS.
    /// The bytes that may start its end tag are held back until it is clear whether they are content.
    fn digest_embedded(
        &mut self,
        e: u8,
        out: &mut impl HTMLWriter,
        text_bytes: &[u8],
        start: &mut usize,
        p: usize,
        end_tag: &[u8],
    ) -> Result<(), HTMLMinifierError> {
        if is_ascii_control(e) {
            self.feed_embedded(&text_bytes[*start..p], out)?;
            *start = p + 1;

            return Ok(());
        }

        let matched = usize::from(self.step_counter);

        if self.script_data.ends(e, end_tag, &mut self.step_counter) {
            self.finish_embedded(out)?;
            out.push_bytes(&self.held[..matched])?;

            // The whitespace after the end tag name is removed.
            *start = if is_whitespace(e) { p + 1 } else { p };

            self.end_raw_text(e);
        } else if usize::from(self.step_counter) > matched {
            if matched == 0 {
                self.feed_embedded(&text_bytes[*start..p], out)?;
            }

            self.held[matched] = e;
            *start = p + 1;
        } else {
            if matched > 0 {
                let held = self.held;
                self.feed_embedded(&held[..matched], out)?;
            }

            if self.step_counter == 1 {
                // This `<` may start the end tag.
                self.held[0] = e;
                *start = p + 1;
            } else {
                self.normalize_line_break(e, out, text_bytes, start, p)?;
            }
        }

        Ok(())
    }

    /// Check whether the byte before `p` is a CR, including the last byte of the previous chunk.
    #[inline]
    fn follows_cr(&self, text_bytes: &[u8], p: usize) -> bool {
        if p == 0 { self.last_cr } else { text_bytes[p - 1] == b'\r' }
    }

    /// Write a CR as a LF and skip a LF that follows a CR, like the HTML input stream does.
    #[inline]
    fn normalize_line_break(
        &mut self,
        e: u8,
        out: &mut impl HTMLWriter,
        text_bytes: &[u8],
        start: &mut usize,
        p: usize,
    ) -> Result<(), HTMLMinifierError> {
        if e == b'\r' {
            self.write_span(&text_bytes[*start..p], out)?;
            self.write_span(b"\n", out)?;
            *start = p + 1;
        } else if e == b'\n' && self.follows_cr(text_bytes, p) {
            self.write_span(&text_bytes[*start..p], out)?;
            *start = p + 1;
        }

        Ok(())
    }

    /// Write the `<!` or `<!-` prefix that was held back while checking for a comment.
    #[inline]
    fn write_declaration_prefix(&self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        match self.step_counter {
            0 => out.push_bytes(b"<!"),
            1 => out.push_bytes(b"<!-"),
            _ => Ok(()),
        }
    }

    #[inline]
    fn push_quoted_value(
        &mut self,
        e: u8,
        out: &mut impl HTMLWriter,
        text_bytes: &[u8],
        start: &mut usize,
        p: usize,
    ) -> Result<(), HTMLMinifierError> {
        if self.quoted_value_empty {
            self.quoted_value_empty = false;
            out.push_bytes(&[b'=', self.quote])?;
        } else if self.quoted_value_spacing {
            out.push_bytes(&text_bytes[*start..p])?;
            *start = p;
            out.push(b' ')?;
        }

        if self.quoted_value_spacing {
            self.push_type_byte(b' ');
        }
        self.push_type_byte(e);

        self.quoted_value_spacing = false;
        Ok(())
    }

    #[inline]
    fn start_preserved(&mut self, root: &'static [u8]) {
        self.preserved_root = root;
        self.preserved_step = PreservedStep::Data;
        self.preserved_depth = 1;
        self.preserved_counter = 0;
        self.preserved_end_tag = false;
        self.preserved_plaintext = false;
        self.preserved_quote = 0;
        self.raw_end = b"";
        self.preserved_tag.clear();
    }

    #[inline]
    fn start_preserved_tag_name(&mut self, e: u8, end_tag: bool) {
        self.preserved_end_tag = end_tag;
        self.preserved_plaintext = false;
        self.preserved_tag.clear();
        self.preserved_tag.push(e.to_ascii_lowercase());
        self.preserved_step = PreservedStep::TagName;
    }

    #[inline]
    fn end_preserved_tag(&mut self) {
        if self.preserved_plaintext {
            self.preserved_plaintext = false;
            self.step = Step::Plaintext;
        }

        self.preserved_step = if self.raw_end.is_empty() {
            PreservedStep::Data
        } else {
            self.preserved_counter = 0;
            PreservedStep::Raw
        };
    }

    /// Follow the attributes of a tag, so that a `>` in a quoted value does not end the tag.
    #[inline]
    fn digest_tag_attribute(&mut self, e: u8) {
        match self.preserved_step {
            PreservedStep::BeforeAttributeName => match e {
                b'>' => self.end_preserved_tag(),
                _ if e == b'/' || is_whitespace(e) => (),
                _ => self.preserved_step = PreservedStep::AttributeName,
            },
            PreservedStep::AttributeName => match e {
                b'>' => self.end_preserved_tag(),
                b'/' => self.preserved_step = PreservedStep::BeforeAttributeName,
                b'=' => self.preserved_step = PreservedStep::BeforeAttributeValue,
                _ if is_whitespace(e) => self.preserved_step = PreservedStep::AfterAttributeName,
                _ => (),
            },
            PreservedStep::AfterAttributeName => match e {
                b'>' => self.end_preserved_tag(),
                b'/' => self.preserved_step = PreservedStep::BeforeAttributeName,
                b'=' => self.preserved_step = PreservedStep::BeforeAttributeValue,
                _ if is_whitespace(e) => (),
                _ => self.preserved_step = PreservedStep::AttributeName,
            },
            PreservedStep::BeforeAttributeValue => match e {
                b'>' => self.end_preserved_tag(),
                b'"' | b'\'' => {
                    self.preserved_quote = e;
                    self.preserved_step = PreservedStep::QuotedAttributeValue;
                },
                _ if is_whitespace(e) => (),
                _ => self.preserved_step = PreservedStep::UnquotedAttributeValue,
            },
            PreservedStep::QuotedAttributeValue => {
                if e == self.preserved_quote {
                    self.preserved_step = PreservedStep::BeforeAttributeName;
                }
            },
            PreservedStep::UnquotedAttributeValue => match e {
                b'>' => self.end_preserved_tag(),
                _ if is_whitespace(e) => self.preserved_step = PreservedStep::BeforeAttributeName,
                _ => (),
            },
            _ => (),
        }
    }

    /// Follow the HTML tokenizer inside a preserved element.
    /// Return `true` when the name of the closing tag of the preserved element ends.
    fn digest_preserved(&mut self, e: u8) -> bool {
        if self.foreign.is_active() {
            self.foreign.feed(e);

            if self.foreign.is_plaintext() {
                self.step = Step::Plaintext;
            } else if self.foreign.breakout_tag() == Some(self.preserved_root) {
                // The HTML start tag that ends the region opens a nested element, such as `<pre>` in `<pre><svg><pre>`.
                self.preserved_depth += 1;
            }

            return false;
        }

        match self.preserved_step {
            PreservedStep::Data => {
                if e == b'<' {
                    self.preserved_step = PreservedStep::TagOpen;
                }
            },
            PreservedStep::TagOpen => match e {
                b'/' => self.preserved_step = PreservedStep::EndTagOpen,
                b'!' => {
                    self.preserved_counter = 0;
                    self.preserved_step = PreservedStep::Declaration;
                },
                b'?' => self.preserved_step = PreservedStep::BogusComment,
                b'<' => (),
                _ if e.is_ascii_alphabetic() => self.start_preserved_tag_name(e, false),
                _ => self.preserved_step = PreservedStep::Data,
            },
            PreservedStep::EndTagOpen => match e {
                b'>' => self.preserved_step = PreservedStep::Data,
                _ if e.is_ascii_alphabetic() => self.start_preserved_tag_name(e, true),
                _ => self.preserved_step = PreservedStep::BogusComment,
            },
            PreservedStep::TagName => {
                if e == b'>' || e == b'/' || is_whitespace(e) {
                    if !self.preserved_end_tag {
                        if matches!(self.preserved_tag.as_slice(), b"svg" | b"math") {
                            self.foreign.start(&self.preserved_tag);
                            self.foreign.feed(e);
                            self.preserved_step = PreservedStep::Data;

                            return false;
                        }

                        self.preserved_plaintext = self.preserved_tag == b"plaintext";
                    }

                    if self.preserved_tag == self.preserved_root {
                        if self.preserved_end_tag {
                            self.preserved_depth -= 1;
                            if self.preserved_depth == 0 {
                                return true;
                            }
                        } else {
                            self.preserved_depth += 1;
                        }
                    }

                    self.raw_end = if self.preserved_end_tag {
                        b""
                    } else {
                        raw_text_end_tag(&self.preserved_tag).unwrap_or_default()
                    };

                    if self.raw_end == b"</script" {
                        self.script_data.reset();
                    }

                    if e == b'>' {
                        self.end_preserved_tag();
                    } else {
                        self.preserved_step = PreservedStep::BeforeAttributeName;
                    }
                } else {
                    push_name_byte(&mut self.preserved_tag, e.to_ascii_lowercase());
                }
            },
            PreservedStep::BeforeAttributeName
            | PreservedStep::AttributeName
            | PreservedStep::AfterAttributeName
            | PreservedStep::BeforeAttributeValue
            | PreservedStep::QuotedAttributeValue
            | PreservedStep::UnquotedAttributeValue => self.digest_tag_attribute(e),
            PreservedStep::Declaration => match e {
                b'-' if self.preserved_counter == 0 => self.preserved_counter = 1,
                b'-' => {
                    self.preserved_counter = COMMENT_START;
                    self.preserved_step = PreservedStep::Comment;
                },
                b'>' => self.preserved_step = PreservedStep::Data,
                _ => self.preserved_step = PreservedStep::BogusComment,
            },
            PreservedStep::BogusComment => {
                if e == b'>' {
                    self.preserved_step = PreservedStep::Data;
                }
            },
            PreservedStep::Comment => match next_comment_state(self.preserved_counter, e) {
                Some(state) => self.preserved_counter = state,
                None => self.preserved_step = PreservedStep::Data,
            },
            PreservedStep::Raw => {
                if self.script_data.ends(e, self.raw_end, &mut self.preserved_counter) {
                    self.raw_end = b"";
                    self.preserved_step = if e == b'>' {
                        PreservedStep::Data
                    } else {
                        PreservedStep::BeforeAttributeName
                    };
                }
            },
        }

        false
    }
}

impl HTMLMinifierHelper {
    /// Reset this HTML minifier helper while keeping its options and allocated memory.
    #[inline]
    pub fn reset(&mut self) {
        self.step = Step::default();
        self.step_counter = 0;
        self.buffer.clear();
        self.tag.clear();
        self.attribute_type.clear();
        self.preserved_tag.clear();
        self.preserved_root = b"";
        self.preserved_step = PreservedStep::default();
        self.css.reset();
        self.js.reset(false);
        self.script_data.reset();
        self.foreign.reset();
        self.preserved_depth = 0;
        self.preserved_counter = 0;
        self.preserved_end_tag = false;
        self.preserved_plaintext = false;
        self.raw_end = b"";
        self.preserved_quote = 0;
        self.quote = 0;
        self.last_space = 0;
        self.last_cr = false;
        self.reference_pending = false;
        self.text_space_written = false;
        self.at_start = false;
        self.quoted_value_spacing = false;
        self.quoted_value_empty = false;
        self.in_handled_attribute = false;
        self.in_attribute_type = false;
        self.in_end_tag = false;
        self.type_seen = false;
        self.type_supported = false;
        self.type_module = false;
    }

    /// Finish the current input and report any incomplete tag or special element.
    /// An error can leave partial output in the writer.
    pub fn finish<W: HTMLWriter>(&mut self, out: &mut W) -> Result<(), HTMLMinifierError> {
        match self.step {
            Step::Initial
            | Step::InitialRemainOneWhitespace
            | Step::InitialIgnoreWhitespace
            | Step::Plaintext => (),
            Step::Foreign if self.foreign.can_finish() => (),
            Step::StartTagInitial => out.push(b'<')?,
            Step::EndTagInitial => out.push_bytes(b"</")?,
            _ => return Err(HTMLMinifierError::IncompleteInput),
        }

        self.reset();
        Ok(())
    }

    /// Input some text to generate HTML code. It is not necessary to input a full HTML text at once.
    pub fn digest<S: AsRef<[u8]>, W: HTMLWriter>(
        &mut self,
        text: S,
        out: &mut W,
    ) -> Result<(), HTMLMinifierError> {
        let text_bytes = text.as_ref();
        let text_length = text_bytes.len();

        let mut start = 0;
        let mut p = 0;

        while p < text_length {
            if self.step.is_special() {
                let next = self.skip_content(text_bytes, p);

                // The bytes that are skipped in a title are not whitespace.
                if next > p && self.in_title() {
                    self.last_space = 0;
                }

                p = next;

                if p == text_length {
                    break;
                }

                let e = text_bytes[p];

                if matches!(self.step, Step::Foreign | Step::Plaintext) {
                    if is_ascii_control(e) {
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    } else {
                        self.normalize_line_break(e, out, text_bytes, &mut start, p)?;

                        if self.step == Step::Foreign {
                            // The bytes that need no change are fed in one run.
                            p = self.foreign.feed_run(text_bytes, p);

                            if self.foreign.is_plaintext() {
                                self.step = Step::Plaintext;
                            } else if let Some(tag) = self.foreign.breakout_tag() {
                                // The HTML start tag that ends the region decides the next step, such as `<pre>` in `<svg><pre>`.
                                self.tag.clear();
                                self.tag.extend_from_slice(tag);

                                self.step = self.end_start_tag_and_get_next_step(
                                    out, text_bytes, &mut start, p,
                                )?;
                            } else if !self.foreign.is_active() {
                                self.last_space = 0;
                                self.step = Step::InitialRemainOneWhitespace;
                            }
                        }
                    }
                } else if matches!(self.step, Step::Pre | Step::Code) {
                    if is_ascii_control(e) {
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    } else if self.digest_preserved(e) {
                        if is_whitespace(e) {
                            out.push_bytes(&text_bytes[start..p])?;
                            start = p + 1;
                        }

                        self.end_raw_text(e);
                    } else {
                        self.normalize_line_break(e, out, text_bytes, &mut start, p)?;
                    }
                } else if let Some(end_tag) = self.step.end_tag(self.raw_end) {
                    if self.step.is_embedded() {
                        self.digest_embedded(e, out, text_bytes, &mut start, p, end_tag)?;
                    } else if is_ascii_control(e) {
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    } else if self.script_data.ends(e, end_tag, &mut self.step_counter) {
                        if is_whitespace(e) {
                            out.push_bytes(&text_bytes[start..p])?;
                            start = p + 1;
                        }

                        self.end_raw_text(e);
                    } else if is_whitespace(e) && self.in_title() {
                        if self.last_space > 0 {
                            // Only the first whitespace of a run is kept in a title.
                            out.push_bytes(&text_bytes[start..p])?;
                            start = p + 1;
                        } else {
                            self.last_space = e;
                            self.normalize_line_break(e, out, text_bytes, &mut start, p)?;
                        }
                    } else {
                        self.last_space = 0;
                        self.normalize_line_break(e, out, text_bytes, &mut start, p)?;
                    }
                }

                p += 1;
                continue;
            }

            let e = text_bytes[p];

            if e <= 0x7F {
                // ASCII
                if is_ascii_control(e) {
                    out.push_bytes(&text_bytes[start..p])?;
                    start = p + 1;
                } else {
                    match self.step {
                        Step::Initial => {
                            // ?
                            match e {
                                b'<' => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.begin_tag();
                                    self.at_start = true;
                                    self.step = Step::StartTagInitial;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;
                                        self.reference_pending = false;
                                    } else {
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                        p = self.skip_plain_text(text_bytes, p);
                                    }
                                },
                            }
                        },
                        Step::InitialRemainOneWhitespace => {
                            // a?
                            if is_whitespace(e) {
                                out.push_bytes(&text_bytes[start..p])?;
                                start = p + 1;

                                self.last_space = e;
                                self.reference_pending = false;

                                self.step = Step::InitialIgnoreWhitespace;
                            } else if e == b'<' {
                                out.push_bytes(&text_bytes[start..p])?;
                                start = p + 1;

                                self.begin_tag();
                                self.step = Step::StartTagInitial;
                            } else {
                                self.last_space = 0;

                                p = self.skip_plain_text(text_bytes, p);
                            }
                        },
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            match e {
                                _ if is_whitespace(e) => {
                                    debug_assert_eq!(start, p);
                                    self.reference_pending = false;

                                    // A line feed in the whitespace makes the kept whitespace a line feed.
                                    loop {
                                        if text_bytes[p] == b'\n' && self.last_space > 0 {
                                            self.last_space = b'\n';
                                        }

                                        match text_bytes.get(p + 1) {
                                            Some(&next) if is_whitespace(next) => p += 1,
                                            _ => break,
                                        }
                                    }

                                    start = p + 1;
                                },
                                b'<' => {
                                    // This can just push ' ', but the minified HTML would be ugly
                                    if self.last_space == b'\n' {
                                        out.push(b'\n')?;
                                    } else if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    self.text_space_written |= self.last_space > 0;
                                    self.last_space = 0;

                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.begin_tag();
                                    self.step = Step::StartTagInitial;
                                },
                                _ => {
                                    if self.last_space == b'\n' {
                                        out.push(b'\n')?;
                                    } else if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                    p = self.skip_plain_text(text_bytes, p);
                                },
                            }
                        },
                        Step::StartTagInitial => {
                            debug_assert_eq!(start, p);

                            // <?
                            match e {
                                b'/' => {
                                    start = p + 1;

                                    self.text_space_written = false;

                                    self.in_end_tag = true;
                                    self.step = Step::EndTagInitial;
                                },
                                b'?' => {
                                    // `<?` starts a bogus comment, which ends at the first `>`.
                                    out.push(b'<')?;

                                    self.reference_pending = false;
                                    self.text_space_written = false;

                                    self.step_counter = 255;
                                    self.step = Step::Doctype;
                                },
                                b'!' => {
                                    // <!
                                    start = p + 1;

                                    self.step_counter = 0;
                                    self.step = Step::Doctype;
                                },
                                _ => {
                                    out.push(b'<')?;

                                    // A literal `<` before another `<` must not join the text after a removed comment or `</>`.
                                    self.reference_pending = e == b'<';
                                    self.text_space_written = false;

                                    if is_whitespace(e) {
                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.last_space = e;

                                        self.step = Step::InitialIgnoreWhitespace;
                                    } else if e.is_ascii_alphabetic() {
                                        self.tag.clear();
                                        self.tag.push(e.to_ascii_lowercase());

                                        self.step = Step::StartTag;
                                    } else {
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                        continue;
                                    }
                                },
                            }
                        },
                        Step::EndTagInitial => {
                            // </?
                            match e {
                                b'>' => {
                                    // </>
                                    if self.reference_pending {
                                        // Keep a boundary so that text on both sides cannot form a character reference or a tag.
                                        out.push_bytes(b"</>")?;
                                        self.reference_pending = false;
                                    }

                                    start = p + 1;

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                },
                                _ => {
                                    out.push_bytes(b"</")?;

                                    self.reference_pending = false;

                                    if e.is_ascii_alphabetic() {
                                        self.step = Step::EndTag;
                                    } else {
                                        // `</` that a letter does not follow starts a bogus comment, which ends at the first `>`.
                                        self.step_counter = 255;
                                        self.step = Step::Doctype;
                                        self.normalize_line_break(
                                            e, out, text_bytes, &mut start, p,
                                        )?;
                                    }
                                },
                            }
                        },
                        Step::StartTag => {
                            // <a?
                            if (is_whitespace(e) || matches!(e, b'/' | b'>'))
                                && matches!(self.tag.as_slice(), b"svg" | b"math")
                            {
                                self.foreign.start(&self.tag);
                                self.step = Step::Foreign;

                                continue;
                            }

                            if is_whitespace(e) {
                                out.push_bytes(&text_bytes[start..p])?;
                                start = p + 1;

                                self.buffer.clear(); // the buffer may be used for the `type` attribute

                                self.last_space = 0;
                                self.step = Step::StartTagIn;
                            } else {
                                match e {
                                    b'/' => self.step = Step::TagEnd,
                                    b'>' => {
                                        self.buffer.clear(); // the buffer may be used for the `type` attribute

                                        self.step = self.end_start_tag_and_get_next_step(
                                            out, text_bytes, &mut start, p,
                                        )?;
                                    },
                                    _ => push_name_byte(&mut self.tag, e.to_ascii_lowercase()),
                                }
                            }
                        },
                        Step::StartTagIn => {
                            // <a ?
                            match e {
                                b'/' => {
                                    if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    self.step = Step::TagEnd;
                                },
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        debug_assert_eq!(start, p);

                                        while text_bytes
                                            .get(p + 1)
                                            .is_some_and(|&next| is_whitespace(next))
                                        {
                                            p += 1;
                                        }

                                        start = p + 1;
                                    } else {
                                        out.push(b' ')?;

                                        self.buffer.clear();
                                        self.buffer.push(e.to_ascii_lowercase());

                                        self.step = Step::StartTagAttributeName;
                                    }
                                },
                            }
                        },
                        Step::StartTagAttributeName => {
                            // <a a?
                            if matches!(e, b'/' | b'>' | b'=') || is_whitespace(e) {
                                self.set_flags_by_attribute();
                            }

                            match e {
                                b'/' => self.step = Step::TagEnd,
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                b'=' => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.step = Step::StartTagAttributeValueInitial;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.step = Step::StartTagAttributeNameWaitingValue;
                                    } else {
                                        push_name_byte(&mut self.buffer, e.to_ascii_lowercase());
                                    }
                                },
                            }
                        },
                        Step::StartTagAttributeNameWaitingValue => {
                            // <a a ?
                            match e {
                                b'/' => self.step = Step::TagEnd,
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                b'=' => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.step = Step::StartTagAttributeValueInitial;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        debug_assert_eq!(start, p);

                                        while text_bytes
                                            .get(p + 1)
                                            .is_some_and(|&next| is_whitespace(next))
                                        {
                                            p += 1;
                                        }

                                        start = p + 1;
                                    } else {
                                        out.push(b' ')?;

                                        self.buffer.clear();
                                        self.buffer.push(e.to_ascii_lowercase());

                                        self.step = Step::StartTagAttributeName;
                                    }
                                },
                            }
                        },
                        Step::StartTagAttributeValueInitial => {
                            // <a a=?
                            debug_assert_eq!(start, p);

                            match e {
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                b'"' | b'\'' => {
                                    self.quoted_value_spacing = false;
                                    self.quoted_value_empty = true;

                                    start = p + 1;

                                    self.quote = e;
                                    self.step = Step::StartTagQuotedAttributeValue;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        start = p + 1;
                                    } else {
                                        self.push_type_byte(e);

                                        out.push(b'=')?;

                                        self.step = Step::StartTagUnquotedAttributeValue;
                                    }
                                },
                            }
                        },
                        Step::StartTagQuotedAttributeValue => {
                            // <a a="?
                            // <a a='?
                            // NOTE: Backslashes cannot be used for escaping.
                            if e == self.quote {
                                if self.quoted_value_empty {
                                    start = p + 1;
                                }

                                self.finish_buffer();

                                out.push_bytes(&text_bytes[start..=p])?;
                                start = p + 1;

                                self.last_space = 0;
                                self.step = Step::StartTagIn;
                            } else if self.in_handled_attribute && is_whitespace(e) {
                                if self.quoted_value_empty {
                                    start = p + 1;
                                } else if self.quoted_value_spacing {
                                    debug_assert_eq!(start, p);
                                    start = p + 1;
                                } else {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.quoted_value_spacing = true;
                                    self.quoted_value_empty = false;
                                }
                            } else {
                                self.push_quoted_value(e, out, text_bytes, &mut start, p)?;
                                self.normalize_line_break(e, out, text_bytes, &mut start, p)?;

                                p = self.skip_quoted_value(text_bytes, p);
                            }
                        },
                        Step::StartTagUnquotedAttributeValue => {
                            // <a a=v?
                            // <a a=v?
                            match e {
                                b'>' => {
                                    self.finish_buffer();
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        self.finish_buffer();

                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.last_space = e;
                                        self.step = Step::StartTagIn;
                                    } else {
                                        self.push_type_byte(e);
                                    }
                                },
                            }
                        },
                        Step::EndTag => {
                            // </a?
                            if is_whitespace(e) {
                                out.push_bytes(&text_bytes[start..p])?;
                                start = p + 1;

                                self.begin_end_tag_attributes();
                            } else if e == b'>' {
                                self.last_space = 0;
                                self.step = Step::InitialRemainOneWhitespace;
                            }
                        },
                        Step::TagEnd => {
                            // <a/?
                            // </a ?
                            // A `>` in a quoted value of an end tag does not end the tag.
                            let quoted = self.in_end_tag
                                && self.preserved_step == PreservedStep::QuotedAttributeValue;

                            match e {
                                b'>' if !quoted => {
                                    if self.in_end_tag {
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    } else {
                                        self.step = self.end_start_tag_and_get_next_step(
                                            out, text_bytes, &mut start, p,
                                        )?;
                                    }
                                },
                                _ => {
                                    out.push_bytes(&text_bytes[start..p])?;

                                    if self.in_end_tag {
                                        start = p + 1;
                                        self.digest_tag_attribute(e);
                                    } else {
                                        // A `/` that `>` does not follow is ignored, so attributes can follow it.
                                        start = p;

                                        self.last_space = 0;
                                        self.step = Step::StartTagIn;
                                        continue;
                                    }
                                },
                            }
                        },
                        Step::Doctype => {
                            // <!?
                            if e == b'>' {
                                self.write_declaration_prefix(out)?;

                                self.reference_pending = false;
                                self.text_space_written = false;

                                self.last_space = 0;
                                self.step = Step::InitialRemainOneWhitespace;
                            } else {
                                match self.step_counter {
                                    0 => match e {
                                        b'-' => {
                                            start = p + 1;

                                            self.step_counter = 1;
                                        },
                                        _ => {
                                            out.push_bytes(b"<!")?;

                                            self.reference_pending = false;
                                            self.text_space_written = false;

                                            self.step_counter = 255;
                                        },
                                    },
                                    1 => match e {
                                        b'-' => {
                                            if !self.remove_comments {
                                                out.push_bytes(b"<!--")?;
                                                self.text_space_written = false;
                                            } else if self.reference_pending {
                                                // Keep a boundary so that text on both sides cannot form a character reference or a tag.
                                                out.push_bytes(b"<!---->")?;
                                            }

                                            self.reference_pending = false;

                                            start = p + 1;

                                            self.step_counter = COMMENT_START;
                                            self.step = Step::Comment;
                                        },
                                        _ => {
                                            out.push_bytes(b"<!-")?;

                                            self.reference_pending = false;
                                            self.text_space_written = false;

                                            self.step_counter = 255;
                                        },
                                    },
                                    255 => (),
                                    _ => unreachable!(),
                                }

                                if self.step_counter == 255 {
                                    self.normalize_line_break(e, out, text_bytes, &mut start, p)?;
                                }
                            }
                        },
                        Step::Comment => {
                            // <!--?
                            if self.remove_comments {
                                debug_assert_eq!(start, p);
                                start = p + 1;
                            } else {
                                self.normalize_line_break(e, out, text_bytes, &mut start, p)?;
                            }

                            match next_comment_state(self.step_counter, e) {
                                Some(state) => self.step_counter = state,
                                None => {
                                    self.step = if self.remove_comments && self.at_start {
                                        // The whitespace after a removed comment at the start of the document is removed too.
                                        Step::Initial
                                    } else if self.remove_comments && self.text_space_written {
                                        // The whitespace before a removed comment is already written, so the whitespace after it is skipped.
                                        Step::InitialIgnoreWhitespace
                                    } else {
                                        Step::InitialRemainOneWhitespace
                                    };
                                    self.last_space = 0;
                                },
                            }
                        },
                        Step::ScriptDefault
                        | Step::ScriptJavaScript
                        | Step::StyleDefault
                        | Step::StyleCSS
                        | Step::Pre
                        | Step::Code
                        | Step::Textarea
                        | Step::RawText
                        | Step::Foreign
                        | Step::Plaintext => unreachable!(),
                    }
                }
            } else {
                // non-ASCII
                match self.step {
                    Step::Initial => {
                        // ?
                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                        p = self.skip_plain_text(text_bytes, p);
                    },
                    Step::InitialRemainOneWhitespace => {
                        // a?
                        self.last_space = 0;

                        p = self.skip_plain_text(text_bytes, p);
                    },
                    Step::InitialIgnoreWhitespace => {
                        // a ?
                        if self.last_space == b'\n' {
                            out.push(b'\n')?;
                        } else if self.last_space > 0 {
                            out.push(b' ')?;
                        }

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                        p = self.skip_plain_text(text_bytes, p);
                    },
                    Step::StartTagInitial => {
                        // <?
                        // To `InitialRemainOneWhitespace`.
                        debug_assert_eq!(start, p);

                        out.push(b'<')?;

                        self.reference_pending = false;
                        self.text_space_written = false;

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    },
                    Step::EndTagInitial => {
                        // </?
                        // `</` that a letter does not follow starts a bogus comment.
                        out.push_bytes(b"</")?;

                        self.reference_pending = false;

                        self.step_counter = 255;
                        self.step = Step::Doctype;
                    },
                    Step::StartTag => push_name_byte(&mut self.tag, e),
                    Step::EndTag => (),
                    Step::StartTagIn => {
                        // <a ?
                        out.push(b' ')?;

                        self.buffer.clear();
                        self.buffer.push(e);

                        self.step = Step::StartTagAttributeName;
                    },
                    Step::StartTagAttributeName => {
                        // <a a?
                        push_name_byte(&mut self.buffer, e);
                    },
                    Step::StartTagAttributeNameWaitingValue => {
                        // <a a ?
                        out.push(b' ')?;

                        self.buffer.clear();
                        self.buffer.push(e);

                        self.step = Step::StartTagAttributeName;
                    },
                    Step::StartTagAttributeValueInitial => {
                        // <a a=?
                        debug_assert_eq!(start, p);

                        self.push_type_byte(e);

                        out.push(b'=')?;

                        self.step = Step::StartTagUnquotedAttributeValue;
                    },
                    Step::StartTagQuotedAttributeValue => {
                        self.push_quoted_value(e, out, text_bytes, &mut start, p)?;

                        p = self.skip_quoted_value(text_bytes, p);
                    },
                    Step::StartTagUnquotedAttributeValue => {
                        // <a a=v?
                        // <a a=v?
                        self.push_type_byte(e);
                    },
                    Step::TagEnd => {
                        // <a/?
                        // </a ?
                        out.push_bytes(&text_bytes[start..p])?;

                        if self.in_end_tag {
                            start = p + 1;
                            self.digest_tag_attribute(e);
                        } else {
                            start = p;

                            self.last_space = 0;
                            self.step = Step::StartTagIn;
                            continue;
                        }
                    },
                    Step::Doctype => {
                        // <!?
                        self.write_declaration_prefix(out)?;

                        self.reference_pending = false;
                        self.text_space_written = false;

                        self.step_counter = 255;
                    },
                    Step::Comment => {
                        // <!--?
                        if self.remove_comments {
                            debug_assert_eq!(start, p);
                            start = p + 1;
                        }

                        self.step_counter = 0;
                    },
                    Step::ScriptDefault
                    | Step::StyleDefault
                    | Step::Pre
                    | Step::Code
                    | Step::Textarea
                    | Step::ScriptJavaScript
                    | Step::StyleCSS
                    | Step::RawText
                    | Step::Foreign
                    | Step::Plaintext => unreachable!(),
                }
            }

            p += 1;
        }

        self.write_span(&text_bytes[start..p], out)?;

        if let Some(&last) = text_bytes.last() {
            self.last_cr = last == b'\r';
        }

        Ok(())
    }
}
