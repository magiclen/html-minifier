extern crate cow_utils;
extern crate minifier;

use core::str::from_utf8_unchecked;

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::functions::*;
use crate::{HTMLMinifierError, HTMLWriter};

use cow_utils::CowUtils;
pub use minifier::{css, js};

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

/// This struct helps you generate and minify your HTML code in the same time. The output destination is outside this struct.
#[derive(Educe, Clone)]
#[educe(Debug, Default(new))]
pub struct HTMLMinifierHelper {
    #[educe(Default = true)]
    /// Remove HTML comments.
    pub remove_comments: bool,
    #[educe(Default = true)]
    /// Minify the content in the `code` element.
    pub minify_code: bool,

    // Buffers
    #[educe(Debug(method = "str_bytes_fmt"))]
    buffer: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    tag: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    attribute_type: Vec<u8>,

    // Steps
    step: Step,
    step_counter: u8,

    // Temp
    quote: u8,
    last_space: u8,

    // Flags
    quoted_value_spacing: bool,
    quoted_value_empty: bool,
    in_handled_attribute: bool,
    in_attribute_type: bool,
}

impl HTMLMinifierHelper {
    #[inline]
    fn set_flags_by_attribute(&mut self) {
        match self.buffer.as_slice() {
            b"class" => {
                self.in_handled_attribute = true;
                self.in_attribute_type = false;
            }
            b"type" => {
                match self.tag.as_slice() {
                    b"script" | b"style" => {
                        self.in_handled_attribute = true;
                        self.in_attribute_type = true;
                    }
                    _ => (),
                }
            }
            _ => {
                self.in_handled_attribute = false;
                self.in_attribute_type = false;
            }
        }
    }

    #[inline]
    fn finish_buffer(&mut self) {
        if self.in_attribute_type {
            if let Cow::Owned(attribute_value) = html_escape::decode_html_entities(unsafe {
                from_utf8_unchecked(&self.attribute_type)
            }) {
                self.attribute_type = attribute_value.into_bytes();
            }

            if let Cow::Owned(attribute_value) =
                unsafe { from_utf8_unchecked(&self.attribute_type) }.cow_to_ascii_lowercase()
            {
                self.attribute_type = attribute_value.into_bytes();
            }
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

                match self.attribute_type.as_slice() {
                    b"" | b"application/javascript" | b"module" => {
                        out.push_bytes(&text_bytes[*start..=p])?;
                        *start = p + 1;

                        self.attribute_type.clear();
                        self.buffer.clear();

                        Step::ScriptJavaScript
                    }
                    _ => {
                        self.attribute_type.clear();

                        Step::ScriptDefault
                    }
                }
            }
            b"style" => {
                self.step_counter = 0;

                match self.attribute_type.as_slice() {
                    b"" | b"text/css" => {
                        out.push_bytes(&text_bytes[*start..=p])?;
                        *start = p + 1;

                        self.attribute_type.clear();
                        self.buffer.clear();

                        Step::StyleCSS
                    }
                    _ => {
                        self.attribute_type.clear();

                        Step::StyleDefault
                    }
                }
            }
            b"pre" => {
                self.step_counter = 0;
                Step::Pre
            }
            b"code" => {
                if self.minify_code {
                    self.last_space = 0;

                    Step::InitialRemainOneWhitespace
                } else {
                    self.step_counter = 0;
                    Step::Code
                }
            }
            b"textarea" => {
                self.step_counter = 0;
                Step::Textarea
            }
            _ => {
                self.last_space = 0;

                Step::InitialRemainOneWhitespace
            }
        };

        Ok(step)
    }
}

impl HTMLMinifierHelper {
    /// Reset this html minifier helper. The option settings and allocated memory will be be preserved.
    #[inline]
    pub fn reset(&mut self) {
        self.step = Step::default();

        self.attribute_type.clear();
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

                                    self.step = Step::StartTagInitial;
                                }
                                _ => {
                                    if is_whitespace(e) {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;
                                    } else {
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                }
                            }
                        }
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

                                self.step = Step::StartTagInitial;
                            } else {
                                self.last_space = 0;
                            }
                        }
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            match e {
                                b'\n' => {
                                    debug_assert_eq!(start, p);
                                    start = p + 1;

                                    if self.last_space > 0 {
                                        self.last_space = b'\n';
                                    }
                                }
                                0x09 | 0x0B..=0x0D | 0x1C..=0x20 => {
                                    debug_assert_eq!(start, p);
                                    start = p + 1;
                                }
                                b'<' => {
                                    // This can just push ' ', but the minified HTML would be ugly
                                    if self.last_space == b'\n' {
                                        out.push(b'\n')?;
                                    } else if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.step = Step::StartTagInitial;
                                }
                                _ => {
                                    if self.last_space == b'\n' {
                                        out.push(b'\n')?;
                                    } else if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
                            }
                        }
                        Step::StartTagInitial => {
                            debug_assert_eq!(start, p);

                            // <?
                            match e {
                                b'/' => {
                                    start = p + 1;

                                    self.step = Step::EndTagInitial;
                                }
                                b'!' => {
                                    // <!
                                    start = p + 1;

                                    self.step_counter = 0;
                                    self.step = Step::Doctype;
                                }
                                b'>' => {
                                    // <>
                                    start = p + 1;

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
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
                                }
                            }
                        }
                        Step::EndTagInitial => {
                            // </?
                            match e {
                                b'>' => {
                                    // </>
                                    start = p + 1;

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
                                _ => {
                                    out.push_bytes(b"</")?;

                                    if is_whitespace(e) {
                                        start = p + 1;

                                        self.last_space = e;

                                        self.step = Step::InitialIgnoreWhitespace;
                                    } else {
                                        self.step = Step::EndTag;
                                    }
                                }
                            }
                        }
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
                                    }
                                    _ => self.tag.push(e.to_ascii_lowercase()),
                                }
                            }
                        }
                        Step::StartTagIn => {
                            // <a ?
                            match e {
                                b'/' => {
                                    if self.last_space > 0 {
                                        out.push(b' ')?;
                                    }

                                    self.step = Step::TagEnd;
                                }
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                }
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
                                }
                            }
                        }
                        Step::StartTagAttributeName => {
                            // <a a?
                            match e {
                                b'/' => self.step = Step::TagEnd,
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                }
                                b'=' => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.set_flags_by_attribute();

                                    self.step = Step::StartTagAttributeValueInitial;
                                }
                                _ => {
                                    if is_whitespace(e) {
                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.step = Step::StartTagAttributeNameWaitingValue;
                                    } else {
                                        self.buffer.push(e.to_ascii_lowercase());
                                    }
                                }
                            }
                        }
                        Step::StartTagAttributeNameWaitingValue => {
                            // <a a ?
                            match e {
                                b'/' => self.step = Step::TagEnd,
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                }
                                b'=' => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;

                                    self.set_flags_by_attribute();

                                    self.step = Step::StartTagAttributeValueInitial;
                                }
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
                                }
                            }
                        }
                        Step::StartTagAttributeValueInitial => {
                            // <a a=?
                            debug_assert_eq!(start, p);

                            match e {
                                b'/' => {
                                    self.step = Step::TagEnd;
                                }
                                b'>' => {
                                    self.step = self.end_start_tag_and_get_next_step(
                                        out, text_bytes, &mut start, p,
                                    )?;
                                }
                                b'"' | b'\'' => {
                                    self.quoted_value_spacing = false;
                                    self.quoted_value_empty = true;

                                    start = p + 1;

                                    self.quote = e;
                                    self.step = Step::StartTagQuotedAttributeValue;
                                }
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
                                }
                            }
                        }
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
                                if self.quoted_value_empty {
                                    self.quoted_value_empty = false;

                                    out.push_bytes(&[b'=', self.quote])?;
                                } else if self.quoted_value_spacing {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p;

                                    out.push(b' ')?;
                                }

                                if self.in_attribute_type {
                                    if self.quoted_value_spacing {
                                        self.attribute_type.push(b' ');
                                    }

                                    self.attribute_type.push(e);
                                }

                                self.quoted_value_spacing = false;
                            }
                        }
                        Step::StartTagUnquotedAttributeValue => {
                            // <a a=v?
                            // <a a=v?
                            match e {
                                b'>' => {
                                    self.finish_buffer();

                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
                                _ => {
                                    if is_whitespace(e) {
                                        self.finish_buffer();

                                        out.push_bytes(&text_bytes[start..p])?;
                                        start = p + 1;

                                        self.last_space = e;
                                        self.step = Step::StartTagIn;
                                    } else if self.in_attribute_type {
                                        self.attribute_type.push(e);
                                    }
                                }
                            }
                        }
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
                        }
                        Step::TagEnd => {
                            // <a/?
                            // </a ?
                            match e {
                                b'>' => {
                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
                                _ => {
                                    out.push_bytes(&text_bytes[start..p])?;
                                    start = p + 1;
                                }
                            }
                        }
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
                                    0 => {
                                        match e {
                                            b'-' => {
                                                start = p + 1;

                                                self.step_counter = 1;
                                            }
                                            _ => {
                                                out.push_bytes(b"<!")?;

                                                self.step_counter = 255;
                                            }
                                        }
                                    }
                                    1 => {
                                        match e {
                                            b'-' => {
                                                if !self.remove_comments {
                                                    out.push_bytes(b"<!--")?;
                                                }

                                                start = p + 1;

                                                self.step_counter = 0;
                                                self.step = Step::Comment;
                                            }
                                            _ => {
                                                out.push_bytes(b"<!-")?;

                                                self.step_counter = 255;
                                            }
                                        }
                                    }
                                    255 => (),
                                    _ => unreachable!(),
                                }
                            }
                        }
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
                                }
                                1 => {
                                    match e {
                                        b'-' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
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
                                        }
                                        _ => self.step_counter = 0,
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::ScriptDefault => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b's' | b'S' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b'c' | b'C' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'r' | b'R' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'i' | b'I' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'p' | b'P' => self.step_counter = 7,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                7 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 8,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                8 => {
                                    match e {
                                        b'>' => {
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                out.push_bytes(&text_bytes[start..p])?;
                                                start = p + 1;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::ScriptJavaScript => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b's' | b'S' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b'c' | b'C' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'r' | b'R' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'i' | b'I' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'p' | b'P' => self.step_counter = 7,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                7 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 8,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                8 => {
                                    match e {
                                        b'>' => {
                                            self.buffer.extend_from_slice(&text_bytes[start..=p]);
                                            start = p + 1;

                                            let script_length = self.buffer.len() - 9;

                                            let minified_js = js::minify(unsafe {
                                                from_utf8_unchecked(&self.buffer[..script_length])
                                            });
                                            out.push_bytes(minified_js.as_bytes())?;
                                            out.push_bytes(&self.buffer[script_length..])?;

                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                self.buffer
                                                    .extend_from_slice(&text_bytes[start..p]);
                                                start = p + 1;

                                                let buffer_length = self.buffer.len();
                                                let script_length = buffer_length - 8;

                                                let minified_js = js::minify(unsafe {
                                                    from_utf8_unchecked(
                                                        &self.buffer[..script_length],
                                                    )
                                                });
                                                out.push_bytes(minified_js.as_bytes())?;
                                                out.push_bytes(&self.buffer[script_length..])?;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::StyleDefault => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b's' | b'S' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'y' | b'Y' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'l' | b'L' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 7,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                7 => {
                                    match e {
                                        b'>' => {
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                out.push_bytes(&text_bytes[start..p])?;
                                                start = p + 1;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::StyleCSS => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b's' | b'S' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'y' | b'Y' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'l' | b'L' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 7,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                7 => {
                                    match e {
                                        b'>' => {
                                            self.buffer.extend_from_slice(&text_bytes[start..=p]);
                                            start = p + 1;

                                            let script_length = self.buffer.len() - 8;

                                            let minified_css = css::minify(unsafe {
                                                from_utf8_unchecked(&self.buffer[..script_length])
                                            })
                                            .map_err(|error| HTMLMinifierError::CSSError(error))?;
                                            out.push_bytes(minified_css.as_bytes())?;
                                            out.push_bytes(&self.buffer[script_length..])?;

                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                self.buffer
                                                    .extend_from_slice(&text_bytes[start..p]);
                                                start = p + 1;

                                                let buffer_length = self.buffer.len();
                                                let script_length = buffer_length - 7;

                                                let minified_css = css::minify(unsafe {
                                                    from_utf8_unchecked(
                                                        &self.buffer[..script_length],
                                                    )
                                                })
                                                .map_err(|error| {
                                                    HTMLMinifierError::CSSError(error)
                                                })?;
                                                out.push_bytes(minified_css.as_bytes())?;
                                                out.push_bytes(&self.buffer[script_length..])?;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::Pre => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b'p' | b'P' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b'r' | b'R' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'>' => {
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                out.push_bytes(&text_bytes[start..p])?;
                                                start = p + 1;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::Code => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b'c' | b'C' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b'o' | b'O' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'd' | b'D' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'>' => {
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                out.push_bytes(&text_bytes[start..p])?;
                                                start = p + 1;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        Step::Textarea => {
                            match self.step_counter {
                                0 => {
                                    if e == b'<' {
                                        self.step_counter = 1;
                                    }
                                }
                                1 => {
                                    match e {
                                        b'/' => self.step_counter = 2,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                2 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 3,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                3 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 4,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                4 => {
                                    match e {
                                        b'x' | b'X' => self.step_counter = 5,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                5 => {
                                    match e {
                                        b't' | b'T' => self.step_counter = 6,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                6 => {
                                    match e {
                                        b'a' | b'A' => self.step_counter = 7,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                7 => {
                                    match e {
                                        b'r' | b'R' => self.step_counter = 8,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                8 => {
                                    match e {
                                        b'e' | b'E' => self.step_counter = 9,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                9 => {
                                    match e {
                                        b'a' | b'A' => self.step_counter = 10,
                                        _ => self.step_counter = 0,
                                    }
                                }
                                10 => {
                                    match e {
                                        b'>' => {
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                        _ => {
                                            if is_whitespace(e) {
                                                out.push_bytes(&text_bytes[start..p])?;
                                                start = p + 1;

                                                self.step = Step::TagEnd;
                                            } else {
                                                self.step_counter = 0;
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }
            } else {
                // non-ASCII
                match self.step {
                    Step::Initial => {
                        // ?
                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    }
                    Step::InitialRemainOneWhitespace => {
                        // a?
                        self.last_space = 0;
                    }
                    Step::InitialIgnoreWhitespace => {
                        // a ?
                        if self.last_space == b'\n' {
                            out.push(b'\n')?;
                        } else if self.last_space > 0 {
                            out.push(b' ')?;
                        }

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    }
                    Step::StartTagInitial => {
                        // <?
                        // To `InitialRemainOneWhitespace`.
                        debug_assert_eq!(start, p);

                        out.push(b'<')?;

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    }
                    Step::EndTagInitial => {
                        // </?
                        // To `InitialRemainOneWhitespace`.
                        out.push_bytes(b"</")?;

                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    }
                    Step::StartTag | Step::EndTag => {
                        // <a?
                        // </a?
                        // To `InitialRemainOneWhitespace`.
                        self.last_space = 0;
                        self.step = Step::InitialRemainOneWhitespace;
                    }
                    Step::StartTagIn => {
                        // <a ?
                        out.push(b' ')?;

                        self.buffer.clear();
                        self.buffer.push(e);

                        self.step = Step::StartTagAttributeName;
                    }
                    Step::StartTagAttributeName => {
                        // <a a?
                        self.buffer.push(e);
                    }
                    Step::StartTagAttributeNameWaitingValue => {
                        // <a a ?
                        out.push(b' ')?;

                        self.buffer.clear();
                        self.buffer.push(e);

                        self.step = Step::StartTagAttributeName;
                    }
                    Step::StartTagAttributeValueInitial => {
                        // <a a=?
                        debug_assert_eq!(start, p);

                        if self.in_attribute_type {
                            self.attribute_type.push(e);
                        }

                        out.push(b'=')?;

                        self.step = Step::StartTagUnquotedAttributeValue;
                    }
                    Step::StartTagQuotedAttributeValue => {
                        // <a a="?
                        // <a a='?
                        if self.quoted_value_empty {
                            self.quoted_value_empty = false;

                            out.push_bytes(&[b'=', self.quote])?;
                        }

                        self.quoted_value_spacing = false;

                        if self.in_attribute_type {
                            self.attribute_type.push(e);
                        }
                    }
                    Step::StartTagUnquotedAttributeValue => {
                        // <a a=v?
                        // <a a=v?
                        if self.in_attribute_type {
                            self.attribute_type.push(e);
                        }
                    }
                    Step::TagEnd => {
                        // <a/?
                        // </a ?
                        out.push_bytes(&text_bytes[start..p])?;
                        start = p + 1;
                    }
                    Step::Doctype => {
                        // <!?
                        if self.step_counter == 0 {
                            out.push_bytes(b"<!")?;
                        }

                        self.step_counter = 255;
                    }
                    Step::Comment => {
                        // <!--?
                        if self.remove_comments {
                            debug_assert_eq!(start, p);
                            start = p + 1;
                        }

                        self.step_counter = 0;
                    }
                    Step::ScriptDefault
                    | Step::StyleDefault
                    | Step::Pre
                    | Step::Code
                    | Step::Textarea
                    | Step::ScriptJavaScript
                    | Step::StyleCSS => {
                        self.step_counter = 0;
                    }
                }
            }

            p += 1;
        }

        match self.step {
            Step::ScriptJavaScript | Step::StyleCSS => {
                self.buffer.extend_from_slice(&text_bytes[start..p]);
            }
            _ => out.push_bytes(&text_bytes[start..p])?,
        }

        Ok(())
    }
}
