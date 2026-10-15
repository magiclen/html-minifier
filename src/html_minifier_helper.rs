use std::str::from_utf8;

use educe::Educe;

use crate::{
    HTMLMinifierError, HTMLWriter, css_minifier::CSSMinifier, functions::*, js_minifier::JSMinifier,
};

/// The longest `type` attribute value that is collected.
/// A longer value is never a supported type.
const MAX_TYPE_LENGTH: usize = 256;

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
            [b"".as_slice(), b"application/javascript", b"text/javascript"]
                .iter()
                .any(|t| value.eq_ignore_ascii_case(t))
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
}

impl Step {
    #[inline]
    const fn end_tag(self) -> Option<&'static [u8]> {
        match self {
            Step::ScriptDefault | Step::ScriptJavaScript => Some(b"</script"),
            Step::StyleDefault | Step::StyleCSS => Some(b"</style"),
            Step::Textarea => Some(b"</textarea"),
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
        )
    }
}

/// Check whether a byte of text content is written as it is.
#[inline]
const fn is_plain_text(e: u8) -> bool {
    !(is_whitespace(e) || e == b'<' || is_ascii_control(e))
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
    /// Remove HTML comments.
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
    css: CSSMinifier,
    js:  JSMinifier,

    // Steps
    step:              Step,
    step_counter:      u8,
    preserved_step:    PreservedStep,
    preserved_depth:   usize,
    preserved_counter: u8,
    preserved_end_tag: bool,
    #[educe(Debug(method = "str_bytes_fmt"))]
    preserved_raw_end: &'static [u8],
    preserved_quote:   u8,

    // Temp
    quote:      u8,
    last_space: u8,
    last_cr:    bool,

    // Flags
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
    /// Reset the state for a new tag.
    #[inline]
    fn begin_tag(&mut self) {
        self.in_end_tag = false;
        self.attribute_type.clear();
        self.type_seen = false;
        self.type_supported = true;
        self.type_module = false;
    }

    #[inline]
    fn set_flags_by_attribute(&mut self) {
        let is_type = self.buffer == b"type" && matches!(self.tag.as_slice(), b"script" | b"style");

        self.in_handled_attribute = is_type || self.buffer == b"class";

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
                self.start_preserved();
                Step::Pre
            },
            b"code" => {
                if self.minify_code {
                    self.last_space = 0;

                    Step::InitialRemainOneWhitespace
                } else {
                    self.start_preserved();
                    Step::Code
                }
            },
            b"textarea" => {
                self.step_counter = 0;
                Step::Textarea
            },
            _ => {
                self.last_space = 0;

                Step::InitialRemainOneWhitespace
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

    /// Skip the bytes of raw text or preserved content that can neither start an end tag nor need a change.
    #[inline]
    fn skip_content(&self, text_bytes: &[u8], mut p: usize) -> usize {
        let plain = if matches!(self.step, Step::Pre | Step::Code) {
            self.preserved_step == PreservedStep::Data
        } else {
            self.step_counter == 0
        };

        // A LF after a CR is removed, so it is not skipped.
        if plain && !self.follows_cr(text_bytes, p) {
            while let Some(&e) = text_bytes.get(p) {
                if e == b'<' || e == b'\r' || is_ascii_control(e) {
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
            self.step = Step::TagEnd;
        }
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

        if is_end_tag(e, end_tag, &mut self.step_counter) {
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
    fn start_preserved(&mut self) {
        self.preserved_step = PreservedStep::Data;
        self.preserved_depth = 1;
        self.preserved_counter = 0;
        self.preserved_end_tag = false;
        self.preserved_quote = 0;
        self.preserved_raw_end = b"";
        self.preserved_tag.clear();
    }

    #[inline]
    fn start_preserved_tag_name(&mut self, e: u8, end_tag: bool) {
        self.preserved_end_tag = end_tag;
        self.preserved_tag.clear();
        self.preserved_tag.push(e.to_ascii_lowercase());
        self.preserved_step = PreservedStep::TagName;
    }

    #[inline]
    fn end_preserved_tag(&mut self) {
        self.preserved_step = if self.preserved_raw_end.is_empty() {
            PreservedStep::Data
        } else {
            self.preserved_counter = 0;
            PreservedStep::Raw
        };
    }

    /// Follow the HTML tokenizer inside a preserved element.
    /// Return `true` when the name of the closing tag of the preserved element ends.
    fn digest_preserved(&mut self, e: u8) -> bool {
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
                    let root: &[u8] = if self.step == Step::Pre { b"pre" } else { b"code" };
                    if self.preserved_tag == root {
                        if self.preserved_end_tag {
                            self.preserved_depth -= 1;
                            if self.preserved_depth == 0 {
                                return true;
                            }
                        } else {
                            self.preserved_depth += 1;
                        }
                    }

                    self.preserved_raw_end = if self.preserved_end_tag {
                        b""
                    } else {
                        match self.preserved_tag.as_slice() {
                            b"script" => b"</script",
                            b"style" => b"</style",
                            b"textarea" => b"</textarea",
                            b"title" => b"</title",
                            b"xmp" => b"</xmp",
                            b"iframe" => b"</iframe",
                            b"noembed" => b"</noembed",
                            b"noframes" => b"</noframes",
                            _ => b"",
                        }
                    };

                    if e == b'>' {
                        self.end_preserved_tag();
                    } else {
                        self.preserved_step = PreservedStep::BeforeAttributeName;
                    }
                } else {
                    self.preserved_tag.push(e.to_ascii_lowercase());
                }
            },
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
            PreservedStep::Declaration => match e {
                b'-' if self.preserved_counter == 0 => self.preserved_counter = 1,
                b'-' => {
                    // Start at 2 so that `<!-->` and `<!--->` end the comment, as in browsers.
                    self.preserved_counter = 2;
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
            PreservedStep::Comment => match e {
                b'-' => self.preserved_counter = (self.preserved_counter + 1).min(2),
                b'>' if self.preserved_counter == 2 => self.preserved_step = PreservedStep::Data,
                _ => self.preserved_counter = 0,
            },
            PreservedStep::Raw => {
                if is_end_tag(e, self.preserved_raw_end, &mut self.preserved_counter) {
                    self.preserved_raw_end = b"";
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
        self.preserved_step = PreservedStep::default();
        self.css.reset();
        self.js.reset(false);
        self.preserved_depth = 0;
        self.preserved_counter = 0;
        self.preserved_end_tag = false;
        self.preserved_raw_end = b"";
        self.preserved_quote = 0;
        self.quote = 0;
        self.last_space = 0;
        self.last_cr = false;
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
            Step::Initial | Step::InitialRemainOneWhitespace | Step::InitialIgnoreWhitespace => (),
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
                p = self.skip_content(text_bytes, p);

                if p == text_length {
                    break;
                }

                let e = text_bytes[p];

                if matches!(self.step, Step::Pre | Step::Code) {
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
                } else if let Some(end_tag) = self.step.end_tag() {
                    if self.step.is_embedded() {
                        self.digest_embedded(e, out, text_bytes, &mut start, p, end_tag)?;
                    } else if is_ascii_control(e) {
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    } else if is_end_tag(e, end_tag, &mut self.step_counter) {
                        if is_whitespace(e) {
                            out.push_bytes(&text_bytes[start..p])?;
                            start = p + 1;
                        }

                        self.end_raw_text(e);
                    } else {
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
                                    self.step = Step::StartTagInitial;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;
                                    } else {
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
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

                                self.step = Step::InitialIgnoreWhitespace;
                            } else if e == b'<' {
                                out.push_bytes(&text_bytes[start..p])?;
                                start = p + 1;

                                self.begin_tag();
                                self.step = Step::StartTagInitial;
                            } else {
                                self.last_space = 0;

                                // Plain text is written as it is.
                                while p + 1 < text_length && is_plain_text(text_bytes[p + 1]) {
                                    p += 1;
                                }
                            }
                        },
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            match e {
                                _ if is_whitespace(e) => {
                                    debug_assert_eq!(start, p);

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
                                },
                            }
                        },
                        Step::StartTagInitial => {
                            debug_assert_eq!(start, p);

                            // <?
                            match e {
                                b'/' => {
                                    start = p + 1;

                                    self.in_end_tag = true;
                                    self.step = Step::EndTagInitial;
                                },
                                b'!' => {
                                    // <!
                                    start = p + 1;

                                    self.step_counter = 0;
                                    self.step = Step::Doctype;
                                },
                                b'>' => {
                                    // <>
                                    start = p + 1;

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                },
                                _ => {
                                    out.push(b'<')?;

                                    if is_whitespace(e) {
                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.last_space = e;

                                        self.step = Step::InitialIgnoreWhitespace;
                                    } else {
                                        self.tag.clear();
                                        self.tag.push(e.to_ascii_lowercase());

                                        self.step = Step::StartTag;
                                    }
                                },
                            }
                        },
                        Step::EndTagInitial => {
                            // </?
                            match e {
                                b'>' => {
                                    // </>
                                    start = p + 1;

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                },
                                _ => {
                                    out.push_bytes(b"</")?;

                                    if is_whitespace(e) {
                                        start = p + 1;

                                        self.last_space = e;

                                        self.step = Step::InitialIgnoreWhitespace;
                                    } else {
                                        self.step = Step::EndTag;
                                    }
                                },
                            }
                        },
                        Step::StartTag => {
                            // <a?
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
                                    _ => self.tag.push(e.to_ascii_lowercase()),
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

                                    self.set_flags_by_attribute();

                                    self.step = Step::StartTagAttributeValueInitial;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.step = Step::StartTagAttributeNameWaitingValue;
                                    } else {
                                        self.buffer.push(e.to_ascii_lowercase());
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

                                    self.set_flags_by_attribute();

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

                                self.step = Step::TagEnd;
                            } else if e == b'>' {
                                self.last_space = 0;
                                self.step = Step::InitialRemainOneWhitespace;
                            }
                        },
                        Step::TagEnd => {
                            // <a/?
                            // </a ?
                            match e {
                                b'>' => {
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
                                    start = p + 1;
                                },
                            }
                        },
                        Step::Doctype => {
                            // <!?
                            if e == b'>' {
                                self.write_declaration_prefix(out)?;

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

                                            self.step_counter = 255;
                                        },
                                    },
                                    1 => match e {
                                        b'-' => {
                                            if !self.remove_comments {
                                                out.push_bytes(b"<!--")?;
                                            }

                                            start = p + 1;

                                            // Start at 2 so that `<!-->` and `<!--->` end the comment, as in browsers.
                                            self.step_counter = 2;
                                            self.step = Step::Comment;
                                        },
                                        _ => {
                                            out.push_bytes(b"<!-")?;

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

                            match e {
                                b'-' => self.step_counter = (self.step_counter + 1).min(2),
                                b'>' if self.step_counter == 2 => {
                                    // The whitespace before a removed comment is already written, so the whitespace after it is skipped.
                                    self.step = if self.remove_comments && self.last_space > 0 {
                                        Step::InitialIgnoreWhitespace
                                    } else {
                                        Step::InitialRemainOneWhitespace
                                    };
                                    self.last_space = 0;
                                },
                                _ => self.step_counter = 0,
                            }
                        },
                        Step::ScriptDefault
                        | Step::ScriptJavaScript
                        | Step::StyleDefault
                        | Step::StyleCSS
                        | Step::Pre
                        | Step::Code
                        | Step::Textarea => unreachable!(),
                    }
                }
            } else {
                // non-ASCII
                match self.step {
                    Step::Initial => {
                        // ?
                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    },
                    Step::InitialRemainOneWhitespace => {
                        // a?
                        self.last_space = 0;

                        // Plain text is written as it is.
                        while p + 1 < text_length && is_plain_text(text_bytes[p + 1]) {
                            p += 1;
                        }
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
                    },
                    Step::StartTagInitial => {
                        // <?
                        // To `InitialRemainOneWhitespace`.
                        debug_assert_eq!(start, p);

                        out.push(b'<')?;

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    },
                    Step::EndTagInitial => {
                        // </?
                        // To `InitialRemainOneWhitespace`.
                        out.push_bytes(b"</")?;

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    },
                    Step::StartTag | Step::EndTag => {
                        // <a?
                        // </a?
                        // To `InitialRemainOneWhitespace`.
                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    },
                    Step::StartTagIn => {
                        // <a ?
                        out.push(b' ')?;

                        self.buffer.clear();
                        self.buffer.push(e);

                        self.step = Step::StartTagAttributeName;
                    },
                    Step::StartTagAttributeName => {
                        // <a a?
                        self.buffer.push(e);
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
                        start = p + 1;
                    },
                    Step::Doctype => {
                        // <!?
                        self.write_declaration_prefix(out)?;

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
                    | Step::StyleCSS => unreachable!(),
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
