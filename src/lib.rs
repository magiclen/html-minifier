/*!
# HTML Minifier

This library can help you generate and minify your HTML code at the same time. It also supports to minify JS and CSS in `<style>`, `<script>` elements, and ignores the minification of `<pre>`, `<code>` and `<textarea>` elements.

HTML is minified by the following rules:

* ASCII control characters (0x00-0x08, 0x11-0x1F, 0x7F) are always removed.
* Comments can be optionally removed. (removed by default)
* **Useless** whitespaces (spaces, tabs and newlines) are removed. (whitespaces between CJ characters are checked)
* Whitespaces (spaces, tabs and newlines) are converted to `'\x20'`, if possible.
* Empty attribute values (e.g value="") are removed.
* The inner HTML of all elements is minified except for the following elements:
    * `<pre>`
    * `<textarea>`
    * `<code>` (optionally, minified by default)
    * `<style>` (if the `type` attribute is unsupported)
    * `<script>` (if the `type` attribute is unsupported)
* JS code and CSS code in `<script>` and `<style>` elements are minified by [minifier](https://crates.io/crates/minifier).

The original (non-minified) HTML doesn't need to be completely generated before using this library because this library doesn't do any deserialization to create DOMs.

## Examples

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<!DOCTYPE html>   <html  ").unwrap();
html_minifier.digest("lang=  en >").unwrap();
html_minifier.digest("
<head>
    <head name=viewport>
</head>
").unwrap();
html_minifier.digest("
<body class=' container   bg-light '>
    <input type='text' value='123   456' readonly=''  />

    123456
    <b>big</b> 789
    ab
    c
    中文
    字
</body>
").unwrap();
html_minifier.digest("</html  >").unwrap();

assert_eq!("<!DOCTYPE html> <html lang=en> <head> <head name=viewport> </head> <body class='container bg-light'> <input type='text' value='123   456' readonly/> 123456 <b>big</b> 789 ab c 中文字 </body> </html>", html_minifier.get_html());
```

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<pre  >   Hello  world!   </pre  >").unwrap();

assert_eq!("<pre>   Hello  world!   </pre>", html_minifier.get_html());
```

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<script type='  application/javascript '>   alert('Hello!')    ;   </script>").unwrap();

assert_eq!("<script type='application/javascript'>alert('Hello!')</script>", html_minifier.get_html());
```

## No Std

Disable the default features to compile this crate without std.

```toml
[dependencies.html-minifier]
version = "*"
default-features = false
```
*/

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[macro_use]
extern crate educe;

extern crate cow_utils;
extern crate minifier;
extern crate utf8_width;

mod errors;

use core::cmp::Ordering;
use core::fmt::{self, Formatter};
use core::str::{from_utf8_unchecked, FromStr};

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use cow_utils::CowUtils;
pub use minifier::{css, js};

pub use errors::*;

#[inline]
fn str_bytes_fmt(v: &[u8], f: &mut Formatter) -> Result<(), fmt::Error> {
    f.write_fmt(format_args!("{:?}", unsafe { from_utf8_unchecked(v) }))
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

/// This struct helps you generate and minify your HTML code in the same time.
#[derive(Educe, Clone)]
#[educe(Debug, Default(new))]
pub struct HTMLMinifier {
    #[educe(Default = true)]
    /// Remove HTML comments.
    pub remove_comments: bool,
    #[educe(Default = true)]
    /// Minify the content in the `code` element.
    pub minify_code: bool,

    // Buffers
    #[educe(Debug(method = "str_bytes_fmt"))]
    out: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    tag: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    attribute_name: Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    buffer: Vec<u8>,

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
    last_cj: bool,
}

#[inline]
fn is_whitespace(e: u8) -> bool {
    match e {
        0x09..=0x0D | 0x1C..=0x20 => true,
        _ => false,
    }
}

#[inline]
fn is_ascii_control(e: u8) -> bool {
    match e {
        0..=8 | 17..=31 | 127 => true,
        _ => false,
    }
}

#[inline]
fn is_cj(c: char) -> bool {
    (c >= '\u{2E80}' && c <= '\u{2EF3}')  // CJK Radicals Supplement
        || (c >= '\u{2F00}' && c <= '\u{2FDF}') // Kangxi Radicals
        || (c >= '\u{2FF0}' && c <= '\u{2FD5}') // Ideographic Description Characters
        || (c >= '\u{3005}' && c <= '\u{303B}') // CJK Symbols and Punctuation
        || (c >= '\u{3040}' && c <= '\u{309F}') // Hiragana
        || (c >= '\u{30A0}' && c <= '\u{30FF}') // Katakana
        || (c >= '\u{3100}' && c <= '\u{312F}') // Bopomofo
        || (c >= '\u{31A0}' && c <= '\u{31BA}') // Bopomofo Extended
        || (c >= '\u{31C0}' && c <= '\u{31E3}') // CJK Strokes
        || (c >= '\u{31F0}' && c <= '\u{31FF}') // Katakana Phonetic Extensions
        || (c >= '\u{32D0}' && c <= '\u{32FF}') // Circled Katakana
        || (c >= '\u{3300}' && c <= '\u{33FF}') // CJK Compatibility
        || (c >= '\u{3400}' && c <= '\u{4DBF}') // CJK Unified Ideographs Extension A
        || (c >= '\u{4E00}' && c <= '\u{9FFF}') // CJK Unified Ideographs
        || (c >= '\u{A000}' && c <= '\u{A48C}') // Yi Syllables
        || (c >= '\u{A490}' && c <= '\u{A4C6}') // Yi Radicals
        || (c >= '\u{F900}' && c <= '\u{FAFF}') // CJK Compatibility Ideographs
        || (c >= '\u{FE30}' && c <= '\u{FE4F}') // CJK Compatibility Forms
        || (c >= '\u{FF65}' && c <= '\u{FF9D}') // Halfwidth And Fullwidth Forms
        || (c >= '\u{1B000}' && c <= '\u{1B0FE}') // Kana Supplement
        || (c >= '\u{1B100}' && c <= '\u{1B11E}') // Kana Extended A
        || (c >= '\u{1B150}' && c <= '\u{1B152}') // Small Kana Extension
        || (c >= '\u{1B164}' && c <= '\u{1B167}') // Small Kana Extension
        || c == '\u{1F200}' // Enclosed Ideographic Supplement
        || (c >= '\u{20000}' && c <= '\u{2A6DF}') // CJK Unified Ideographs Extension B
        || (c >= '\u{2A700}' && c <= '\u{2B73F}') // CJK Unified Ideographs Extension C
        || (c >= '\u{2B740}' && c <= '\u{2B81F}') // CJK Unified Ideographs Extension D
        || (c >= '\u{2B820}' && c <= '\u{2CEAF}') // CJK Unified Ideographs Extension E
        || (c >= '\u{2CEB0}' && c <= '\u{2EBEF}') // CJK Unified Ideographs Extension F
        || (c >= '\u{2F800}' && c <= '\u{2FA1F}') // CJK Compatibility Ideographs Supplement
}

#[inline]
fn is_bytes_cj(bytes: &[u8]) -> bool {
    match char::from_str(unsafe { from_utf8_unchecked(bytes) }) {
        Ok(c) => is_cj(c),
        Err(_) => false,
    }
}

impl HTMLMinifier {
    #[inline]
    fn remove(&mut self, text_bytes: &[u8], start: usize, p: usize, count: usize) {
        let buffer_length = p - start;

        match buffer_length.cmp(&count) {
            Ordering::Equal => (),
            Ordering::Greater => self.out.extend_from_slice(&text_bytes[start..(p - count)]),
            Ordering::Less => unsafe {
                self.out.set_len(self.out.len() - (count - buffer_length));
            },
        }
    }

    #[inline]
    fn set_flags_by_attribute(&mut self) {
        match self.attribute_name.as_slice() {
            b"class" => {
                self.in_handled_attribute = true;
                self.in_attribute_type = false;
            }
            b"type" => {
                match self.tag.as_slice() {
                    b"script" | b"style" => {
                        self.in_handled_attribute = true;
                        self.in_attribute_type = true;
                        self.buffer.clear();
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
            if let Cow::Owned(attribute_value) =
                html_escape::decode_html_entities(unsafe { from_utf8_unchecked(&self.buffer) })
            {
                self.buffer = attribute_value.into_bytes();
            }

            if let Cow::Owned(attribute_value) =
                unsafe { from_utf8_unchecked(&self.buffer) }.cow_to_ascii_lowercase()
            {
                self.buffer = attribute_value.into_bytes();
            }
        }
    }

    #[inline]
    fn end_start_tag_and_get_next_step(
        &mut self,
        text_bytes: &[u8],
        start: &mut usize,
        p: usize,
    ) -> Step {
        match self.tag.as_slice() {
            b"script" => {
                self.step_counter = 0;

                match self.buffer.as_slice() {
                    b"" | b"application/javascript" => {
                        self.out.extend_from_slice(&text_bytes[*start..=p]);
                        *start = p + 1;

                        self.buffer.clear();

                        Step::ScriptJavaScript
                    }
                    _ => Step::ScriptDefault,
                }
            }
            b"style" => {
                self.step_counter = 0;

                match self.buffer.as_slice() {
                    b"" | b"text/css" => {
                        self.out.extend_from_slice(&text_bytes[*start..=p]);
                        *start = p + 1;

                        self.buffer.clear();

                        Step::StyleCSS
                    }
                    _ => Step::StyleDefault,
                }
            }
            b"pre" => {
                self.step_counter = 0;
                Step::Pre
            }
            b"code" => {
                if self.minify_code {
                    self.last_cj = false;
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
                self.last_cj = false;
                self.last_space = 0;

                Step::InitialRemainOneWhitespace
            }
        }
    }
}

impl HTMLMinifier {
    /// Reset this html minifier. The option settings will be be preserved.
    #[inline]
    pub fn reset(&mut self) {
        self.out.clear();
        self.step = Step::default();
    }

    /// Input some text to generate HTML code. It is not necessary to input a full HTML text at once.
    pub fn digest<S: AsRef<str>>(&mut self, text: S) -> Result<(), HTMLMinifierError> {
        let text = text.as_ref();
        let text_bytes = text.as_bytes();
        let text_length = text_bytes.len();

        self.out.reserve(text_length);

        let mut start = 0;
        let mut p = 0;

        while p < text_length {
            let e = text_bytes[p];

            let width = unsafe { utf8_width::get_width_assume_valid(e) };

            match width {
                1 => {
                    let e = text_bytes[p];

                    if is_ascii_control(e) {
                        self.out.extend_from_slice(&text_bytes[start..p]);
                        start = p + 1;
                    } else {
                        match self.step {
                            Step::Initial => {
                                // ?
                                match e {
                                    b'<' => self.step = Step::StartTagInitial,
                                    _ => {
                                        if is_whitespace(e) {
                                            debug_assert_eq!(start, p);
                                            start = p + 1;
                                        } else {
                                            self.last_cj = false;
                                            self.last_space = 0;
                                            self.step = Step::InitialRemainOneWhitespace;
                                        }
                                    }
                                }
                            }
                            Step::InitialRemainOneWhitespace => {
                                // a?
                                if is_whitespace(e) {
                                    self.out.extend_from_slice(&text_bytes[start..p]);
                                    start = p + 1;

                                    self.last_space = e;

                                    self.step = Step::InitialIgnoreWhitespace;
                                } else if e == b'<' {
                                    self.step = Step::StartTagInitial;
                                } else {
                                    self.last_cj = false;
                                    self.last_space = 0;
                                }
                            }
                            Step::InitialIgnoreWhitespace => {
                                // a ?
                                match e {
                                    b'\n' => {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;

                                        self.last_space = b'\n';
                                    }
                                    0x09 | 0x0B..=0x0D | 0x1C..=0x20 => {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;
                                    }
                                    b'<' => {
                                        if self.last_space > 0 {
                                            self.out.push(b' ');
                                        }

                                        self.step = Step::StartTagInitial;
                                    }
                                    _ => {
                                        if self.last_space > 0 {
                                            self.out.push(b' ');
                                        }

                                        self.last_cj = false;
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                }
                            }
                            Step::StartTagInitial => {
                                // <?
                                match e {
                                    b'/' => self.step = Step::EndTagInitial,
                                    b'!' => {
                                        // <!
                                        self.step_counter = 0;
                                        self.step = Step::Doctype;
                                    }
                                    b'>' => {
                                        // <>
                                        self.remove(text_bytes, start, p, 1);
                                        start = p + 1;

                                        self.last_cj = false;
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            self.out.extend_from_slice(&text_bytes[start..p]);
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
                                        self.remove(text_bytes, start, p, 2);
                                        start = p + 1;

                                        self.last_cj = false;
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            self.out.extend_from_slice(&text_bytes[start..p]);
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
                                    self.out.extend_from_slice(&text_bytes[start..p]);
                                    start = p + 1;

                                    self.buffer.clear(); // the buffer may be used for the `type` attribute

                                    self.step = Step::StartTagIn;
                                } else {
                                    match e {
                                        b'/' => self.step = Step::TagEnd,
                                        b'>' => {
                                            self.buffer.clear(); // the buffer may be used for the `type` attribute

                                            self.step = self.end_start_tag_and_get_next_step(
                                                text_bytes, &mut start, p,
                                            )
                                        }
                                        _ => self.tag.push(e.to_ascii_lowercase()),
                                    }
                                }
                            }
                            Step::StartTagIn => {
                                // <a ?
                                match e {
                                    b'/' => self.step = Step::TagEnd,
                                    b'>' => {
                                        self.step = self.end_start_tag_and_get_next_step(
                                            text_bytes, &mut start, p,
                                        )
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            debug_assert_eq!(start, p);
                                            start = p + 1;
                                        } else {
                                            self.out.push(b' ');

                                            self.attribute_name.clear();
                                            self.attribute_name.push(e.to_ascii_lowercase());

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
                                            text_bytes, &mut start, p,
                                        )
                                    }
                                    b'=' => {
                                        self.set_flags_by_attribute();

                                        self.step = Step::StartTagAttributeValueInitial;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            self.out.extend_from_slice(&text_bytes[start..p]);
                                            start = p + 1;

                                            self.step = Step::StartTagAttributeNameWaitingValue;
                                        } else {
                                            self.attribute_name.push(e.to_ascii_lowercase());
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
                                            text_bytes, &mut start, p,
                                        )
                                    }
                                    b'=' => {
                                        self.set_flags_by_attribute();

                                        self.step = Step::StartTagAttributeValueInitial;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            debug_assert_eq!(start, p);
                                            start = p + 1;
                                        } else {
                                            self.out.push(b' ');

                                            self.attribute_name.clear();
                                            self.attribute_name.push(e.to_ascii_lowercase());

                                            self.step = Step::StartTagAttributeName;
                                        }
                                    }
                                }
                            }
                            Step::StartTagAttributeValueInitial => {
                                // <a a=?
                                match e {
                                    b'/' => {
                                        self.remove(text_bytes, start, p, 1);
                                        start = p;

                                        self.step = Step::TagEnd;
                                    }
                                    b'>' => {
                                        self.remove(text_bytes, start, p, 1);
                                        start = p;

                                        self.step = self.end_start_tag_and_get_next_step(
                                            text_bytes, &mut start, p,
                                        );
                                    }
                                    b'"' | b'\'' => {
                                        self.quoted_value_spacing = false;
                                        self.quoted_value_empty = true;

                                        self.quote = e;
                                        self.step = Step::StartTagQuotedAttributeValue;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            self.out.extend_from_slice(&text_bytes[start..p]);
                                            start = p + 1;
                                        } else {
                                            if self.in_attribute_type {
                                                self.buffer.push(e);
                                            }

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
                                        self.remove(text_bytes, start, p, 2);
                                        start = p + 1;
                                    } else if self.quoted_value_spacing {
                                        self.remove(text_bytes, start, p, 1);
                                        start = p;

                                        if self.in_attribute_type {
                                            unsafe {
                                                self.buffer.set_len(self.buffer.len() - 1);
                                            }
                                        }
                                    }
                                    self.finish_buffer();

                                    self.out.extend_from_slice(&text_bytes[start..=p]);
                                    start = p + 1;

                                    self.step = Step::StartTagIn;
                                } else if self.in_handled_attribute && is_whitespace(e) {
                                    if self.quoted_value_empty {
                                        self.out.extend_from_slice(&text_bytes[start..p]);
                                        start = p + 1;
                                    } else if self.quoted_value_spacing {
                                        debug_assert_eq!(start, p);
                                        start = p + 1;
                                    } else {
                                        self.out.extend_from_slice(&text_bytes[start..p]);
                                        start = p + 1;
                                        self.out.push(b' ');
                                        if self.in_attribute_type {
                                            self.buffer.push(b' ');
                                        }

                                        self.quoted_value_spacing = true;
                                        self.quoted_value_empty = false;
                                    }
                                } else {
                                    self.quoted_value_spacing = false;
                                    self.quoted_value_empty = false;

                                    if self.in_attribute_type {
                                        self.buffer.push(e);
                                    }
                                }
                            }
                            Step::StartTagUnquotedAttributeValue => {
                                // <a a=v?
                                // <a a=v?
                                match e {
                                    b'>' => {
                                        self.finish_buffer();

                                        self.last_cj = false;
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                    _ => {
                                        if is_whitespace(e) {
                                            self.finish_buffer();

                                            self.out.extend_from_slice(&text_bytes[start..p]);
                                            start = p + 1;

                                            self.step = Step::StartTagIn;
                                        } else if self.in_attribute_type {
                                            self.buffer.push(e);
                                        }
                                    }
                                }
                            }
                            Step::EndTag => {
                                // </a?
                                if is_whitespace(e) {
                                    self.out.extend_from_slice(&text_bytes[start..p]);
                                    start = p + 1;

                                    self.step = Step::TagEnd;
                                } else if e == b'>' {
                                    self.last_cj = false;
                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                }
                            }
                            Step::TagEnd => {
                                // <a/?
                                // </a ?
                                match e {
                                    b'>' => {
                                        self.last_cj = false;
                                        self.last_space = 0;
                                        self.step = Step::InitialRemainOneWhitespace;
                                    }
                                    _ => {
                                        self.out.extend_from_slice(&text_bytes[start..p]);
                                        start = p + 1;
                                    }
                                }
                            }
                            Step::Doctype => {
                                // <!?
                                if e == b'>' {
                                    self.last_cj = false;
                                    self.last_space = 0;
                                    self.step = Step::InitialRemainOneWhitespace;
                                } else {
                                    match self.step_counter {
                                        0 => {
                                            match e {
                                                b'-' => self.step_counter = 1,
                                                _ => self.step_counter = 255,
                                            }
                                        }
                                        1 => {
                                            match e {
                                                b'-' => {
                                                    if self.remove_comments {
                                                        if self.last_space > 0 {
                                                            //  <!--
                                                            self.remove(text_bytes, start, p, 4);
                                                        } else {
                                                            // <!--
                                                            self.remove(text_bytes, start, p, 3);
                                                        }
                                                    } else {
                                                        self.out.extend_from_slice(
                                                            &text_bytes[start..=p],
                                                        );
                                                    }
                                                    start = p + 1;

                                                    self.step_counter = 0;
                                                    self.step = Step::Comment;
                                                }
                                                _ => self.step_counter = 255,
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
                                                    self.step = Step::InitialIgnoreWhitespace;
                                                } else {
                                                    // No need to set the `last_cj` and `last_space`.
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
                                                self.last_cj = false;
                                                self.last_space = 0;
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                            _ => {
                                                if is_whitespace(e) {
                                                    self.out
                                                        .extend_from_slice(&text_bytes[start..p]);
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
                                                self.buffer
                                                    .extend_from_slice(&text_bytes[start..=p]);
                                                start = p + 1;

                                                let script_length = self.buffer.len() - 9;

                                                let minified_js = js::minify(unsafe {
                                                    from_utf8_unchecked(
                                                        &self.buffer[..script_length],
                                                    )
                                                });
                                                self.out.extend_from_slice(minified_js.as_bytes());
                                                self.out.extend_from_slice(
                                                    &self.buffer[script_length..],
                                                );

                                                self.last_cj = false;
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
                                                    self.out
                                                        .extend_from_slice(minified_js.as_bytes());
                                                    self.out.extend_from_slice(
                                                        &self.buffer[script_length..],
                                                    );

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
                                                self.last_cj = false;
                                                self.last_space = 0;
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                            _ => {
                                                if is_whitespace(e) {
                                                    self.out
                                                        .extend_from_slice(&text_bytes[start..p]);
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
                                                self.buffer
                                                    .extend_from_slice(&text_bytes[start..=p]);
                                                start = p + 1;

                                                let script_length = self.buffer.len() - 8;

                                                let minified_css = css::minify(unsafe {
                                                    from_utf8_unchecked(
                                                        &self.buffer[..script_length],
                                                    )
                                                })
                                                .map_err(|error| {
                                                    HTMLMinifierError::CSSError(error)
                                                })?;
                                                self.out.extend_from_slice(minified_css.as_bytes());
                                                self.out.extend_from_slice(
                                                    &self.buffer[script_length..],
                                                );

                                                self.last_cj = false;
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
                                                    self.out
                                                        .extend_from_slice(minified_css.as_bytes());
                                                    self.out.extend_from_slice(
                                                        &self.buffer[script_length..],
                                                    );

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
                                                self.last_cj = false;
                                                self.last_space = 0;
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                            _ => {
                                                if is_whitespace(e) {
                                                    self.out
                                                        .extend_from_slice(&text_bytes[start..p]);
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
                                                self.last_cj = false;
                                                self.last_space = 0;
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                            _ => {
                                                if is_whitespace(e) {
                                                    self.out
                                                        .extend_from_slice(&text_bytes[start..p]);
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
                                                self.last_cj = false;
                                                self.last_space = 0;
                                                self.step = Step::InitialRemainOneWhitespace;
                                            }
                                            _ => {
                                                if is_whitespace(e) {
                                                    self.out
                                                        .extend_from_slice(&text_bytes[start..p]);
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
                }
                2 => {
                    match self.step {
                        Step::Initial => {
                            // ?
                            self.last_cj = false;
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::InitialRemainOneWhitespace => {
                            // a?
                            self.last_cj = false;
                            self.last_space = 0;
                        }
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            if self.last_space > 0 {
                                self.out.push(b' ');
                            }

                            self.last_cj = false;
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::StartTagInitial
                        | Step::EndTagInitial
                        | Step::StartTag
                        | Step::EndTag => {
                            // <?
                            // </?
                            // <a?
                            // </a?
                            // To `InitialRemainOneWhitespace`.
                            self.last_cj = false;
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::StartTagIn => {
                            // <a ?
                            self.out.push(b' ');

                            self.attribute_name.clear();
                            self.attribute_name.push(e);
                            self.attribute_name.push(text_bytes[p + 1]);

                            self.step = Step::StartTagAttributeName;
                        }
                        Step::StartTagAttributeName => {
                            // <a a?
                            self.attribute_name.push(e);
                            self.attribute_name.push(text_bytes[p + 1]);
                        }
                        Step::StartTagAttributeNameWaitingValue => {
                            // <a a ?
                            self.out.push(b' ');

                            self.attribute_name.clear();
                            self.attribute_name.push(e);
                            self.attribute_name.push(text_bytes[p + 1]);

                            self.step = Step::StartTagAttributeName;
                        }
                        Step::StartTagAttributeValueInitial => {
                            // <a a=?
                            if self.in_attribute_type {
                                self.buffer.push(e);
                                self.buffer.push(text_bytes[p + 1]);
                            }

                            self.step = Step::StartTagUnquotedAttributeValue;
                        }
                        Step::StartTagQuotedAttributeValue => {
                            // <a a="?
                            // <a a='?
                            self.quoted_value_spacing = false;
                            self.quoted_value_empty = false;

                            if self.in_attribute_type {
                                self.buffer.push(e);
                                self.buffer.push(text_bytes[p + 1]);
                            }
                        }
                        Step::StartTagUnquotedAttributeValue => {
                            // <a a=v?
                            // <a a=v?
                            if self.in_attribute_type {
                                self.buffer.push(e);
                                self.buffer.push(text_bytes[p + 1]);
                            }
                        }
                        Step::TagEnd => {
                            // <a/?
                            // </a ?
                            self.out.extend_from_slice(&text_bytes[start..p]);
                            start = p + 2;
                        }
                        Step::Doctype => {
                            // <!?
                            self.step_counter = 255;
                        }
                        Step::Comment => {
                            // <!--?
                            if self.remove_comments {
                                debug_assert_eq!(start, p);
                                start = p + 2;
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
                _ => {
                    match self.step {
                        Step::Initial => {
                            // ?
                            self.last_cj = is_bytes_cj(&text_bytes[p..(p + width)]);
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::InitialRemainOneWhitespace => {
                            // a?
                            self.last_cj = is_bytes_cj(&text_bytes[p..(p + width)]);
                            self.last_space = 0;
                        }
                        Step::InitialIgnoreWhitespace => {
                            // a ?
                            let cj = is_bytes_cj(&text_bytes[p..(p + width)]);

                            if self.last_space > 0
                                && (self.last_space != b'\n' || !(cj && self.last_cj))
                            {
                                self.out.push(b' ');
                            }

                            self.last_cj = cj;
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::StartTagInitial
                        | Step::EndTagInitial
                        | Step::StartTag
                        | Step::EndTag => {
                            // <?
                            // </?
                            // <a?
                            // </a?
                            // To `InitialRemainOneWhitespace`.
                            self.last_cj = false;
                            self.last_space = 0;
                            self.step = Step::InitialRemainOneWhitespace;
                        }
                        Step::StartTagIn => {
                            // <a ?
                            self.out.push(b' ');

                            self.attribute_name.clear();
                            self.attribute_name.extend_from_slice(&text_bytes[p..(p + width)]);

                            self.step = Step::StartTagAttributeName;
                        }
                        Step::StartTagAttributeName => {
                            // <a a?
                            self.attribute_name.extend_from_slice(&text_bytes[p..(p + width)]);
                        }
                        Step::StartTagAttributeNameWaitingValue => {
                            // <a a ?
                            self.out.push(b' ');

                            self.attribute_name.clear();
                            self.attribute_name.extend_from_slice(&text_bytes[p..(p + width)]);

                            self.step = Step::StartTagAttributeName;
                        }
                        Step::StartTagAttributeValueInitial => {
                            // <a a=?
                            if self.in_attribute_type {
                                self.buffer.extend_from_slice(&text_bytes[p..(p + width)]);
                            }

                            self.step = Step::StartTagUnquotedAttributeValue;
                        }
                        Step::StartTagQuotedAttributeValue => {
                            // <a a="?
                            // <a a='?
                            self.quoted_value_spacing = false;
                            self.quoted_value_empty = false;

                            if self.in_attribute_type {
                                self.buffer.extend_from_slice(&text_bytes[p..(p + width)]);
                            }
                        }
                        Step::StartTagUnquotedAttributeValue => {
                            // <a a=v?
                            // <a a=v?
                            if self.in_attribute_type {
                                self.buffer.extend_from_slice(&text_bytes[p..(p + width)]);
                            }
                        }
                        Step::TagEnd => {
                            // <a/?
                            // </a ?
                            self.out.extend_from_slice(&text_bytes[start..p]);
                            start = p + width;
                        }
                        Step::Doctype => {
                            // <!?
                            self.step_counter = 255;
                        }
                        Step::Comment => {
                            // <!--?
                            if self.remove_comments {
                                debug_assert_eq!(start, p);
                                start = p + width;
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
            }

            p += width;
        }

        match self.step {
            Step::ScriptJavaScript | Step::StyleCSS => {
                self.buffer.extend_from_slice(&text_bytes[start..p]);
            }
            _ => self.out.extend_from_slice(&text_bytes[start..p]),
        }

        Ok(())
    }

    /// Get HTML in a string slice.
    #[inline]
    pub fn get_html(&mut self) -> &str {
        unsafe { from_utf8_unchecked(self.out.as_slice()) }
    }
}

/// Minify HTML.
#[inline]
pub fn minify<S: AsRef<str>>(html: S) -> Result<String, HTMLMinifierError> {
    let mut minifier = HTMLMinifier::new();

    minifier.digest(html.as_ref())?;

    Ok(String::from(minifier.get_html()))
}
