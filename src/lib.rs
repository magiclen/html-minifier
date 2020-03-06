/*!
# HTML Minifier
This tool can help you generate and minify your HTML code at the same time. It also supports to minify JS and CSS in `<style>`, `<script>` elements, and ignores the minification of `<pre>`, `<code>` and `<textarea>` elements.

HTML is minified by the following rules:

* Removal of ascii control characters (0x00-0x08, 0x11-0x1F, 0x7F).
* Removal of comments. (Optional)
* Removal of **unused** multiple whitespaces(spaces, tabs and newlines).
* Minification of CSS code in `<style>` elements by using [minifier](https://crates.io/crates/minifier).
* Minification of JS code in `<script>` elements by using [minifier](https://crates.io/crates/minifier).
* Prevention of minifing `<pre>`, `<code>` and `<textarea>` elements.

You should notice that the HTML code is generated and minified simultaneously, which means you don't need an extra space to store you original HTML source.

## Examples

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest(r#"
                <!DOCTYPE html>
                <html lang=en>
                    <  head>
                        <head  name=viewport  >
                    </head  >
                    <body     class="container    bg-light" >
                        <input type="text" value='123   456'    />
                        <!-- Content -->
                        123456 <b>big</b> 789

                    <  /body>
                </  html>
        "#).unwrap();

assert_eq!(r#"<!DOCTYPE html> <html lang=en> <head> <head name=viewport> </head> <body class="container bg-light"> <input type="text" value='123   456'/> 123456 <b>big</b> 789 </body> </html>"#, html_minifier.get_html());
```

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest(r#"<pre   lang="html"  >
    <html>
        1234567
    </html></pre>
    <div>
        1234567
    </div>
    <pre>
        1234567
    </pre>"#).unwrap();

assert_eq!(r#"<pre lang="html">
    <html>
        1234567
    </html></pre> <div> 1234567 </div> <pre>
        1234567
    </pre>"#, html_minifier.get_html());
```

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest(r#"<script>
        alert('1234!')    ;

        </script>"#).unwrap();

assert_eq!("<script>alert('1234!')</script>", html_minifier.get_html());
```

```rust
extern crate html_minifier;

use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest(r#"<style>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
p  {
    color: red;
    font-family: courier;
    font-size: 160%;
}
        </style>"#).unwrap();

assert_eq!("<style>h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>", html_minifier.get_html());
```
*/

extern crate minifier;

#[macro_use]
extern crate educe;

pub use minifier::css;
pub use minifier::js;

/// This struct helps you generate and minify your HTML code in the same time.
#[derive(Debug, Educe, Clone)]
#[educe(Default(new))]
pub struct HTMLMinifier {
    #[educe(Default = true)]
    /// Remove HTML comments.
    pub remove_comments: bool,

    // Buffers
    out: Vec<char>,
    buffer: Vec<char>,
    start_tag: Vec<char>,
    end_tag: Vec<char>,
    attribute: String,
    attribute_value: String,

    // Counters
    tag_counter: u8,

    // Temp
    attribute_quote: char,
    handled_attribute: bool,
    saved_attribute: bool,

    // Flags
    #[educe(Default = true)]
    ignoring_space: bool,
    last_space: bool,
    last_new_line: bool,
    comment_last_space: bool,
    in_starting_tagging: bool,
    in_start_tagging: bool,
    in_end_tagging: bool,
    in_attribute: bool,
    is_tagging: bool,
    is_start_tagging: bool,
    is_just_finish_tagging: bool,
    is_comment: bool,
    in_pre_tag: bool,
    in_code_tag: bool,
    in_textarea_tag: bool,
    in_script_tag: bool,
    in_js_tag: bool,
    in_style_tag: bool,
    in_css_tag: bool,
}

#[inline]
fn is_space_or_new_line(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n'
}

#[inline]
fn is_ascii_control(c: char) -> bool {
    (c >= '\0' && c <= '\x08') || (c >= '\x11' && c <= '\x1F') || c == '\x7F'
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

macro_rules! into_tag {
    ($s:expr) => {{
        let len = $s.start_tag.len();

        if len == 3 {
            let tag = &$s.start_tag;

            if tag[0].to_ascii_lowercase() == 'p'
                && tag[1].to_ascii_lowercase() == 'r'
                && tag[2].to_ascii_lowercase() == 'e'
            {
                $s.in_pre_tag = true;
                $s.tag_counter = 0;
            }
        } else if len == 4 {
            let tag = &$s.start_tag;

            if tag[0].to_ascii_lowercase() == 'c'
                && tag[1].to_ascii_lowercase() == 'o'
                && tag[2].to_ascii_lowercase() == 'd'
                && tag[3].to_ascii_lowercase() == 'e'
            {
                $s.in_code_tag = true;
                $s.tag_counter = 0;
            }
        } else if len == 5 {
            let tag = &$s.start_tag;

            if tag[0].to_ascii_lowercase() == 's'
                && tag[1].to_ascii_lowercase() == 't'
                && tag[2].to_ascii_lowercase() == 'y'
                && tag[3].to_ascii_lowercase() == 'l'
                && tag[4].to_ascii_lowercase() == 'e'
            {
                match $s.attribute_value.as_str() {
                    "" | "text/css" => {
                        $s.in_css_tag = true;
                        $s.buffer.clear();
                        $s.tag_counter = 0;
                    }
                    _ => {
                        $s.in_style_tag = true;
                        $s.tag_counter = 0;
                    }
                }
            }
        } else if len == 6 {
            let tag = &$s.start_tag;

            if tag[0].to_ascii_lowercase() == 's'
                && tag[1].to_ascii_lowercase() == 'c'
                && tag[2].to_ascii_lowercase() == 'r'
                && tag[3].to_ascii_lowercase() == 'i'
                && tag[4].to_ascii_lowercase() == 'p'
                && tag[5].to_ascii_lowercase() == 't'
            {
                match $s.attribute_value.as_str() {
                    "" | "application/javascript" => {
                        $s.in_js_tag = true;
                        $s.buffer.clear();
                        $s.tag_counter = 0;
                    }
                    _ => {
                        $s.in_script_tag = true;
                        $s.tag_counter = 0;
                    }
                }
            }
        } else if len == 8 {
            let tag = &$s.start_tag;

            if tag[0].to_ascii_lowercase() == 't'
                && tag[1].to_ascii_lowercase() == 'e'
                && tag[2].to_ascii_lowercase() == 'x'
                && tag[3].to_ascii_lowercase() == 't'
                && tag[4].to_ascii_lowercase() == 'a'
                && tag[5].to_ascii_lowercase() == 'r'
                && tag[6].to_ascii_lowercase() == 'e'
                && tag[7].to_ascii_lowercase() == 'a'
            {
                $s.in_textarea_tag = true;
                $s.tag_counter = 0;
            }
        }
    }};
}

impl HTMLMinifier {
    #[inline]
    /// Reset this html minifier in order to input a HTML text. The option settings will be maintained.
    pub fn reset(&mut self) {
        self.out.clear();
        self.buffer.clear();

        self.tag_counter = 0;

        self.ignoring_space = true;
        self.last_space = false;
        self.last_new_line = false;
        self.in_starting_tagging = false;
        self.in_start_tagging = false;
        self.in_end_tagging = false;
        self.in_attribute = false;
        self.is_tagging = false;
        self.is_start_tagging = false;
        self.is_just_finish_tagging = false;
        self.is_comment = false;
    }

    /// Input some text to generate HTML code. You don't need to input a full HTML text at once.
    #[allow(clippy::cognitive_complexity)]
    pub fn digest<S: AsRef<str>>(&mut self, text: S) -> Result<(), &'static str> {
        for c in text.as_ref().chars() {
            if is_ascii_control(c) {
                continue;
            } else if self.is_comment {
                if c == '-' {
                    self.tag_counter += 1;
                } else if c == '>' && self.tag_counter >= 2 {
                    self.is_comment = false;
                }

                if self.remove_comments {
                    continue;
                }
            } else if self.in_pre_tag {
                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 'p' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 'r' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c == '>' {
                    self.in_pre_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_space_or_new_line(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e -= 1;
                    }

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 5 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }
            } else if self.in_code_tag {
                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 'c' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 'o' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'd' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c == '>' {
                    self.in_code_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_space_or_new_line(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e -= 1;
                    }

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 6 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }
            } else if self.in_textarea_tag {
                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'x' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c.to_ascii_lowercase() == 'a' {
                    self.tag_counter = 7;
                } else if self.tag_counter == 7 && c.to_ascii_lowercase() == 'r' {
                    self.tag_counter = 8;
                } else if self.tag_counter == 8 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 9;
                } else if self.tag_counter == 9 && c.to_ascii_lowercase() == 'a' {
                    self.tag_counter = 10;
                } else if self.tag_counter == 10 && c == '>' {
                    self.in_textarea_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_space_or_new_line(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e -= 1;
                    }

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 10 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }
            } else if self.in_js_tag {
                self.buffer.push(c);

                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 'c' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'r' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 'i' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c.to_ascii_lowercase() == 'p' {
                    self.tag_counter = 7;
                } else if self.tag_counter == 7 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 8;
                } else if self.tag_counter == 8 && c == '>' {
                    self.in_js_tag = false;

                    let buffer = &mut self.buffer;
                    let mut temp = Vec::new();

                    let mut e = buffer.len() - 1;

                    loop {
                        let c = *buffer.get(e).unwrap();

                        buffer.remove(e);

                        if c == '<' {
                            temp.insert(0, '<');
                            break;
                        } else if !is_space_or_new_line(c) {
                            temp.insert(0, c);
                        }

                        e -= 1;
                    }

                    let minified_js = js::minify(&buffer.iter().collect::<String>());
                    buffer.clear();

                    self.out.extend(minified_js.chars());

                    self.out.extend(temp);

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 8 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }

                continue;
            } else if self.in_script_tag {
                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 'c' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'r' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 'i' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c.to_ascii_lowercase() == 'p' {
                    self.tag_counter = 7;
                } else if self.tag_counter == 7 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 8;
                } else if self.tag_counter == 8 && c == '>' {
                    self.in_script_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_space_or_new_line(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e -= 1;
                    }

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 8 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }
            } else if self.in_css_tag {
                self.buffer.push(c);

                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'y' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 'l' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 7;
                } else if self.tag_counter == 7 && c == '>' {
                    self.in_css_tag = false;

                    let buffer = &mut self.buffer;
                    let mut temp = Vec::new();

                    let mut e = buffer.len() - 1;

                    loop {
                        let c = *buffer.get(e).unwrap();

                        buffer.remove(e);

                        if c == '<' {
                            temp.insert(0, '<');
                            break;
                        } else if !is_space_or_new_line(c) {
                            temp.insert(0, c);
                        }

                        e -= 1;
                    }

                    let minified_css = css::minify(&buffer.iter().collect::<String>())?;
                    buffer.clear();

                    self.out.extend(minified_css.chars());

                    self.out.extend(temp);

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 7 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }

                continue;
            } else if self.in_style_tag {
                if c == '<' {
                    if self.tag_counter == 0 {
                        self.tag_counter = 1;
                    }
                } else if self.tag_counter == 1 && c == '/' {
                    self.tag_counter = 2;
                } else if self.tag_counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.tag_counter = 3;
                } else if self.tag_counter == 3 && c.to_ascii_lowercase() == 't' {
                    self.tag_counter = 4;
                } else if self.tag_counter == 4 && c.to_ascii_lowercase() == 'y' {
                    self.tag_counter = 5;
                } else if self.tag_counter == 5 && c.to_ascii_lowercase() == 'l' {
                    self.tag_counter = 6;
                } else if self.tag_counter == 6 && c.to_ascii_lowercase() == 'e' {
                    self.tag_counter = 7;
                } else if self.tag_counter == 7 && c == '>' {
                    self.in_style_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_space_or_new_line(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e -= 1;
                    }

                    self.is_just_finish_tagging = true;
                } else if self.tag_counter == 1 || self.tag_counter == 2 || self.tag_counter == 7 {
                    if !is_space_or_new_line(c) {
                        self.tag_counter = 0;
                    }
                } else {
                    self.tag_counter = 0;
                }
            } else if is_space_or_new_line(c) {
                if self.ignoring_space {
                    continue;
                }

                if self.in_start_tagging {
                    self.in_start_tagging = false;
                    self.buffer.clear();
                } else if self.in_end_tagging {
                    self.in_end_tagging = false;
                    self.buffer.clear();
                } else if self.is_tagging {
                    if self.in_attribute {
                        if self.handled_attribute {
                            self.out.push(' ');

                            self.ignoring_space = true;
                        } else {
                            self.out.push(c);

                            continue;
                        }
                    } else {
                        self.buffer.clear();
                    }
                }

                if c == '\n' {
                    self.last_new_line = true;
                }

                self.last_space = true;

                continue;
            } else if c == '<' && !self.is_tagging {
                self.is_tagging = true;
                self.is_start_tagging = true;

                self.in_starting_tagging = true;

                self.comment_last_space = false;

                self.attribute_value.clear();

                if self.last_space && self.out.last().is_some() {
                    self.out.push(' ');
                    self.comment_last_space = true;
                }

                self.ignoring_space = true;
                self.last_space = false;
                self.last_new_line = false;
            } else if self.in_starting_tagging {
                self.in_starting_tagging = false;

                if c == '>' {
                    self.is_tagging = false;
                    self.is_just_finish_tagging = true;
                } else if c == '/' {
                    self.is_start_tagging = false;

                    self.in_end_tagging = true;

                    self.end_tag.clear();
                } else {
                    if c == '!' {
                        // It may be a comment.
                        self.tag_counter = 1;
                    } else {
                        self.tag_counter = 0;
                    }

                    self.ignoring_space = false;
                    self.in_start_tagging = true;

                    self.start_tag.clear();
                    self.start_tag.push(c);
                }
            } else if self.in_start_tagging {
                if c == '>' {
                    self.in_start_tagging = false;

                    self.is_tagging = false;
                    self.is_just_finish_tagging = true;

                    into_tag!(self);
                } else if c == '/' {
                    self.in_start_tagging = false;
                } else {
                    if self.tag_counter > 0 {
                        if c == '-' {
                            self.tag_counter += 1;
                        }

                        if self.tag_counter == 3 {
                            self.in_start_tagging = false;
                            self.is_tagging = false;
                            self.is_comment = true;
                            self.tag_counter = 0;

                            if self.remove_comments {
                                let len = self.out.len();

                                if self.comment_last_space {
                                    unsafe {
                                        self.out.set_len(len - 4);
                                    }
                                } else {
                                    unsafe {
                                        self.out.set_len(len - 3);
                                    }
                                }
                            } else {
                                self.out.push(c);
                            }

                            continue;
                        }
                    }

                    self.start_tag.push(c);
                }
            } else if self.in_end_tagging {
                if c == '>' {
                    self.in_end_tagging = false;

                    self.is_tagging = false;
                    self.is_just_finish_tagging = true;
                } else {
                    self.end_tag.push(c);
                }

                self.ignoring_space = false;
                self.last_space = false;
                self.last_new_line = false;
            } else if self.is_tagging {
                if self.in_attribute {
                    if c == self.attribute_quote {
                        let last = *self.out.last().unwrap();

                        if last != '\\' {
                            self.in_attribute = false;

                            if self.handled_attribute && last == ' ' {
                                self.out.remove(self.out.len() - 1);
                            }

                            if self.saved_attribute {
                                self.attribute_value =
                                    self.buffer.iter().collect::<String>().to_lowercase();
                                self.buffer.clear();
                            }
                        }
                    }

                    if self.saved_attribute {
                        self.buffer.push(c);
                    }

                    self.ignoring_space = false;
                } else if c == '>' {
                    self.is_tagging = false;
                    self.is_just_finish_tagging = true;

                    if self.is_start_tagging {
                        into_tag!(self);
                    }
                } else if c == '/' {
                    // do nothing
                } else if c == '"' || c == '\'' {
                    self.in_attribute = true;

                    self.attribute_quote = c;

                    match self.attribute.as_str() {
                        "class" => {
                            self.handled_attribute = true;
                            self.ignoring_space = true;
                        }
                        "type" => {
                            self.handled_attribute = true;
                            self.saved_attribute = true;
                            self.ignoring_space = true;
                        }
                        _ => {
                            self.handled_attribute = false;
                            self.saved_attribute = false;
                        }
                    }
                } else if c == '=' {
                    self.attribute = self.buffer.iter().collect::<String>().to_lowercase();
                    self.buffer.clear();
                } else {
                    if self.last_space {
                        if let Some(&last) = self.out.last() {
                            if self.last_new_line {
                                if !is_cj(last) || !is_cj(c) {
                                    self.out.push(' ');
                                }
                            } else {
                                self.out.push(' ');
                            }
                        }
                    }

                    self.buffer.push(c);
                }

                self.last_space = false;
                self.last_new_line = false;
            } else {
                if self.last_space {
                    if let Some(&last) = self.out.last() {
                        if self.last_new_line {
                            if !is_cj(last) || !is_cj(c) {
                                self.out.push(' ');
                            }
                        } else {
                            self.out.push(' ');
                        }
                    }
                }

                self.ignoring_space = false;
                self.last_space = false;
                self.last_new_line = false;
            }

            self.out.push(c);
        }

        Ok(())
    }

    /// Generate HTML code into a `String` instance.
    #[inline]
    pub fn get_html(&mut self) -> String {
        self.out.iter().collect()
    }

    /// Minify HTML.
    #[inline]
    pub fn minify<S: AsRef<str>>(html: S) -> Result<String, &'static str> {
        let mut minifier = HTMLMinifier::new();

        minifier.digest(html.as_ref())?;

        Ok(minifier.get_html())
    }
}

/// Minify HTML.
pub fn minify<S: AsRef<str>>(html: S) -> Result<String, &'static str> {
    HTMLMinifier::minify(html)
}
