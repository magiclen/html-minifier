use std::{borrow::Cow, str::from_utf8};

use educe::Educe;
pub use minifier::{css, js};
use str_utils::ToLowercase;

use crate::{HTMLMinifierError, HTMLWriter, functions::*};

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
    const fn is_buffered(self) -> bool {
        matches!(self, Step::ScriptJavaScript | Step::StyleCSS)
    }
}

#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum PreservedStep {
    #[educe(Default)]
    Data,
    TagOpen,
    TagName,
    TagBody,
    Declaration,
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
    /// Minify JavaScript in supported script elements, which is disabled by default.
    pub minify_js:       bool,
    /// Minify CSS in supported style elements, which is disabled by default.
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

    // Steps
    step:              Step,
    step_counter:      u8,
    preserved_step:    PreservedStep,
    preserved_depth:   usize,
    preserved_counter: u8,
    preserved_end_tag: bool,
    preserved_raw_end: &'static [u8],
    preserved_quote:   u8,

    // Temp
    quote:      u8,
    last_space: u8,

    // Flags
    quoted_value_spacing: bool,
    quoted_value_empty:   bool,
    in_handled_attribute: bool,
    in_attribute_type:    bool,
    in_end_tag:           bool,
}

impl HTMLMinifierHelper {
    #[inline]
    fn set_flags_by_attribute(&mut self) {
        self.in_attribute_type =
            self.buffer == b"type" && matches!(self.tag.as_slice(), b"script" | b"style");
        self.in_handled_attribute = self.buffer == b"class" || self.in_attribute_type;

        if self.in_attribute_type {
            self.attribute_type.clear();
        }
    }

    #[inline]
    fn finish_buffer(&mut self) -> Result<(), HTMLMinifierError> {
        if self.in_attribute_type {
            if let Cow::Owned(attribute_value) =
                html_escape::decode_html_entities(from_utf8(&self.attribute_type)?)
            {
                self.attribute_type = attribute_value.into_bytes();
            }

            if let Cow::Owned(attribute_value) =
                from_utf8(&self.attribute_type)?.to_ascii_lowercase_cow()
            {
                self.attribute_type = attribute_value.into_bytes();
            }
        }

        Ok(())
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

                match self.attribute_type.as_slice() {
                    b"" | b"application/javascript" | b"module" if self.minify_js => {
                        out.push_bytes(&text_bytes[*start..=p])?;
                        *start = p + 1;

                        self.attribute_type.clear();
                        self.buffer.clear();

                        Step::ScriptJavaScript
                    },
                    _ => {
                        self.attribute_type.clear();

                        Step::ScriptDefault
                    },
                }
            },
            b"style" => {
                self.step_counter = 0;

                match self.attribute_type.as_slice() {
                    b"" | b"text/css" if self.minify_css => {
                        out.push_bytes(&text_bytes[*start..=p])?;
                        *start = p + 1;

                        self.attribute_type.clear();
                        self.buffer.clear();

                        Step::StyleCSS
                    },
                    _ => {
                        self.attribute_type.clear();

                        Step::StyleDefault
                    },
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
        if self.step.is_buffered() {
            self.buffer.extend_from_slice(bytes);
            Ok(())
        } else {
            out.push_bytes(bytes)
        }
    }

    fn write_minified(
        &mut self,
        length: usize,
        out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        let text = from_utf8(&self.buffer[..length])?;
        let minified = if self.step == Step::ScriptJavaScript {
            js::minify(text).map_err(HTMLMinifierError::JSError)?.to_string()
        } else {
            css::minify(text).map_err(HTMLMinifierError::CSSError)?.to_string()
        };

        out.push_bytes(minified.as_bytes())?;
        out.push_bytes(&self.buffer[length..])?;
        self.buffer.clear();

        Ok(())
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

        if self.in_attribute_type {
            if self.quoted_value_spacing {
                self.attribute_type.push(b' ');
            }
            self.attribute_type.push(e);
        }

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
    fn end_preserved_tag(&mut self, e: u8) {
        self.preserved_step = if e == b'>' {
            if self.preserved_raw_end.is_empty() {
                PreservedStep::Data
            } else {
                self.preserved_counter = 0;
                PreservedStep::Raw
            }
        } else {
            PreservedStep::TagBody
        };
    }

    fn digest_preserved(&mut self, e: u8) -> bool {
        match self.preserved_step {
            PreservedStep::Data => {
                if e == b'<' {
                    self.preserved_step = PreservedStep::TagOpen;
                    self.preserved_raw_end = b"";
                    self.preserved_quote = 0;
                }
            },
            PreservedStep::TagOpen => match e {
                b'/' => {
                    self.preserved_end_tag = true;
                    self.preserved_tag.clear();
                    self.preserved_step = PreservedStep::TagName;
                },
                b'!' => {
                    self.preserved_counter = 0;
                    self.preserved_step = PreservedStep::Declaration;
                },
                b'<' => (),
                _ if e.is_ascii_alphabetic() => {
                    self.preserved_end_tag = false;
                    self.preserved_tag.clear();
                    self.preserved_tag.push(e.to_ascii_lowercase());
                    self.preserved_step = PreservedStep::TagName;
                },
                _ => self.preserved_step = PreservedStep::Data,
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

                    if !self.preserved_end_tag {
                        self.preserved_raw_end = match self.preserved_tag.as_slice() {
                            b"script" => b"</script",
                            b"style" => b"</style",
                            b"textarea" => b"</textarea",
                            b"title" => b"</title",
                            b"xmp" => b"</xmp",
                            b"iframe" => b"</iframe",
                            b"noembed" => b"</noembed",
                            b"noframes" => b"</noframes",
                            _ => b"",
                        };
                    }
                    self.end_preserved_tag(e);
                } else {
                    self.preserved_tag.push(e.to_ascii_lowercase());
                }
            },
            PreservedStep::TagBody => {
                if self.preserved_quote != 0 {
                    if e == self.preserved_quote {
                        self.preserved_quote = 0;
                    }
                } else if e == b'\'' || e == b'"' {
                    self.preserved_quote = e;
                } else if e == b'>' {
                    self.end_preserved_tag(e);
                }
            },
            PreservedStep::Declaration => {
                if e == b'-' && self.preserved_counter < 2 {
                    self.preserved_counter += 1;
                    if self.preserved_counter == 2 {
                        self.preserved_counter = 0;
                        self.preserved_step = PreservedStep::Comment;
                    }
                } else if e == b'>' {
                    self.preserved_step = PreservedStep::Data;
                } else {
                    self.preserved_quote = if e == b'\'' || e == b'"' { e } else { 0 };
                    self.preserved_step = PreservedStep::TagBody;
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
                    self.preserved_quote = 0;
                    self.end_preserved_tag(e);
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
        self.preserved_depth = 0;
        self.preserved_counter = 0;
        self.preserved_end_tag = false;
        self.preserved_raw_end = b"";
        self.preserved_quote = 0;
        self.quote = 0;
        self.last_space = 0;
        self.quoted_value_spacing = false;
        self.quoted_value_empty = false;
        self.in_handled_attribute = false;
        self.in_attribute_type = false;
        self.in_end_tag = false;
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
            let e = text_bytes[p];

            if matches!(self.step, Step::Pre | Step::Code) {
                if is_ascii_control(e) {
                    out.push_bytes(&text_bytes[start..p])?;
                    start = p + 1;
                } else if self.digest_preserved(e) {
                    self.last_space = 0;
                    if e == b'>' {
                        self.step = Step::InitialRemainOneWhitespace;
                    } else {
                        if is_whitespace(e) {
                            out.push_bytes(&text_bytes[start..p])?;
                            start = p + 1;
                        }
                        self.in_end_tag = true;
                        self.step = Step::TagEnd;
                    }
                }

                p += 1;
                continue;
            }

            if let Some(end_tag) = self.step.end_tag() {
                if is_ascii_control(e) {
                    self.write_span(&text_bytes[start..p], out)?;
                    start = p + 1;
                } else if is_end_tag(e, end_tag, &mut self.step_counter) {
                    let include_end = e == b'>' || e == b'/';
                    if self.step.is_buffered() {
                        let end = p + usize::from(include_end);
                        self.buffer.extend_from_slice(&text_bytes[start..end]);
                        start = p + 1;
                        let length = self.buffer.len() - end_tag.len() - usize::from(include_end);
                        self.write_minified(length, out)?;
                    } else if is_whitespace(e) {
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    }

                    self.last_space = 0;
                    if e == b'>' {
                        self.step = Step::InitialRemainOneWhitespace;
                    } else {
                        self.in_end_tag = true;
                        self.step = Step::TagEnd;
                    }
                }

                p += 1;
                continue;
            }

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

                                    self.in_end_tag = false;
                                    self.attribute_type.clear();
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

                                self.in_end_tag = false;
                                self.attribute_type.clear();
                                self.step = Step::StartTagInitial;
                            } else {
                                self.last_space = 0;
                            }
                        },
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            match e {
                                b'\n' => {
                                    debug_assert_eq!(start, p);
                                    start = p + 1;

                                    if self.last_space > 0 {
                                        self.last_space = b'\n';
                                    }
                                },
                                0x09 | 0x0B..=0x0D | 0x1C..=0x20 => {
                                    debug_assert_eq!(start, p);
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

                                    self.in_end_tag = false;
                                    self.attribute_type.clear();
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
                                        if self.in_attribute_type {
                                            self.attribute_type.push(e);
                                        }

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

                                self.finish_buffer()?;

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
                            }
                        },
                        Step::StartTagUnquotedAttributeValue => {
                            // <a a=v?
                            // <a a=v?
                            match e {
                                b'>' => {
                                    self.finish_buffer()?;
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                },
                                _ => {
                                    if is_whitespace(e) {
                                        self.finish_buffer()?;

                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.last_space = e;
                                        self.step = Step::StartTagIn;
                                    } else if self.in_attribute_type {
                                        self.attribute_type.push(e);
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
                                if self.step_counter == 0 {
                                    out.push_bytes(b"<!")?;
                                }

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

                                            self.step_counter = 0;
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
                            }
                        },
                        Step::Comment => {
                            // <!--?
                            if self.remove_comments {
                                debug_assert_eq!(start, p);
                                start = p + 1;
                            }

                            match self.step_counter {
                                0 => {
                                    if e == b'-' {
                                        self.step_counter = 1;
                                    }
                                },
                                1 => match e {
                                    b'-' => self.step_counter = 2,
                                    _ => self.step_counter = 0,
                                },
                                2 => {
                                    match e {
                                        b'>' => {
                                            if self.last_space > 0 {
                                                self.last_space = 0;

                                                self.step = Step::InitialIgnoreWhitespace;
                                            } else {
                                                // No need to set `last_space`.
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                        },
                                        _ => self.step_counter = 0,
                                    }
                                },
                                _ => unreachable!(),
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

                        if self.in_attribute_type {
                            self.attribute_type.push(e);
                        }

                        out.push(b'=')?;

                        self.step = Step::StartTagUnquotedAttributeValue;
                    },
                    Step::StartTagQuotedAttributeValue => {
                        self.push_quoted_value(e, out, text_bytes, &mut start, p)?;
                    },
                    Step::StartTagUnquotedAttributeValue => {
                        // <a a=v?
                        // <a a=v?
                        if self.in_attribute_type {
                            self.attribute_type.push(e);
                        }
                    },
                    Step::TagEnd => {
                        // <a/?
                        // </a ?
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    },
                    Step::Doctype => {
                        // <!?
                        if self.step_counter == 0 {
                            out.push_bytes(b"<!")?;
                        }

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

        Ok(())
    }
}
