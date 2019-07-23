/*!
# HTML Minifier
This tool can help you generate and minify your HTML code at the same time. It also supports to minify JS and CSS in `<style>`, `<script>` elements, and ignores the minification of `<pre>` elements.

HTML is minified by the following rules:

* Removal of ascii control characters (0x00-0x08, 0x11-0x1F, 0x7F).
* Removal of comments.
* Removal of **unused** multiple whitespaces(spaces, tabs and newlines).
* Minification of CSS code in `<style>` elements by using [minifier](https://crates.io/crates/minifier).
* Minification of JS code in `<script>` elements by using [minifier](https://crates.io/crates/minifier).
* Prevention of minifing `<pre>` elements.

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

assert_eq!(r#"<!DOCTYPE html><html lang=en><head><head name=viewport></head><body class="container bg-light"><input type="text" value='123   456'/>123456 <b>big</b> 789</body></html>"#, html_minifier.get_html());
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
    </html></pre><div>1234567</div><pre>
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

pub use minifier::js;
pub use minifier::css;

/// This struct helps you generate and minify your HTML code in the same time.
#[derive(Debug)]
pub struct HTMLMinifier {
    out: Vec<char>,
    buffer: Vec<char>,
    ignoring_space: bool,
    start_tag: Vec<char>,
    end_tag: Vec<char>,
    attribute: String,
    in_starting_tagging: bool,
    in_start_tagging: bool,
    in_end_tagging: bool,
    in_pre_tag: bool,
    in_js_tag: bool,
    in_css_tag: bool,
    in_attribute: bool,
    attribute_quote: char,
    last_space: bool,
    line_space: bool,
    counter: u8,
    is_comment: bool,
    is_tagging: bool,
    is_start_tagging: bool,
    is_just_finish_tagging: bool,
}

macro_rules! is_space_or_new_line {
    ( $c:expr ) => {
        {
            $c == ' ' || $c == '\t' || $c == '\n'
        }
    };
}

macro_rules! is_ascii_control {
    ( $c:expr ) => {
        {
            ($c >= '\0' && $c <= '\x08') || ($c >= '\x11' && $c <= '\x1f') || $c == '\x7f'
        }
    };
}

macro_rules! into_tag {
    ( $s:expr ) => {
        {
            let len = $s.start_tag.len();

            if len == 3 {
                let tag = &$s.start_tag;

                if tag[0].to_ascii_lowercase() == 'p' && tag[1].to_ascii_lowercase() == 'r' && tag[2].to_ascii_lowercase() == 'e' {
                    $s.in_pre_tag = true;
                    $s.counter = 0;
                }
            } else if len == 5 {
                let tag = &$s.start_tag;

                if tag[0].to_ascii_lowercase() == 's' && tag[1].to_ascii_lowercase() == 't' && tag[2].to_ascii_lowercase() == 'y' && tag[3].to_ascii_lowercase() == 'l' && tag[4].to_ascii_lowercase() == 'e' {
                    $s.in_css_tag = true;
                    $s.buffer.clear();
                    $s.counter = 0;
                }
            } else if len == 6 {
                let tag = &$s.start_tag;

                if tag[0].to_ascii_lowercase() == 's' && tag[1].to_ascii_lowercase() == 'c' && tag[2].to_ascii_lowercase() == 'r' && tag[3].to_ascii_lowercase() == 'i' && tag[4].to_ascii_lowercase() == 'p' && tag[5].to_ascii_lowercase() == 't' {
                    $s.in_js_tag = true;
                    $s.buffer.clear();
                    $s.counter = 0;
                }
            }
        }
    };
}

impl HTMLMinifier {
    /// Create a new HTMLMinifier instance.
    pub fn new() -> HTMLMinifier {
        HTMLMinifier {
            out: Vec::new(),
            buffer: Vec::new(),
            ignoring_space: true,
            start_tag: Vec::new(),
            end_tag: Vec::new(),
            attribute: "".to_string(),
            in_starting_tagging: false,
            in_start_tagging: false,
            in_end_tagging: false,
            in_pre_tag: false,
            in_js_tag: false,
            in_css_tag: false,
            in_attribute: false,
            attribute_quote: ' ',
            last_space: false,
            line_space: false,
            counter: 0,
            is_comment: false,
            is_tagging: false,
            is_start_tagging: false,
            is_just_finish_tagging: false,
        }
    }

    /// Input some text to generate HTML code. You don't need to input a full HTML text at once.
    pub fn digest<S: AsRef<str>>(&mut self, text: S) -> Result<(), &'static str> {
        for c in text.as_ref().chars() {
            if is_ascii_control!(c) {
                continue;
            } else if self.is_comment {
                if c == '-' {
                    self.counter = self.counter + 1;
                } else if c == '>' {
                    if self.counter >= 2 {
                        self.is_comment = false;
                    }
                }
                continue;
            } else if self.in_pre_tag {
                if c == '<' {
                    if self.counter == 0 {
                        self.counter = 1;
                    }
                } else if self.counter == 1 && c == '/' {
                    self.counter = 2;
                } else if self.counter == 2 && c.to_ascii_lowercase() == 'p' {
                    self.counter = 3;
                } else if self.counter == 3 && c.to_ascii_lowercase() == 'r' {
                    self.counter = 4;
                } else if self.counter == 4 && c.to_ascii_lowercase() == 'e' {
                    self.counter = 5;
                } else if self.counter == 5 && c == '>' {
                    self.in_pre_tag = false;

                    let out = &mut self.out;

                    let mut e = out.len() - 1;

                    loop {
                        let c = *out.get(e).unwrap();

                        if is_ascii_control!(c) {
                            out.remove(e);
                        } else if c == '<' {
                            break;
                        }

                        e = e - 1;
                    }

                    self.is_just_finish_tagging = true;
                } else {
                    if self.counter == 1 || self.counter == 2 || self.counter == 5 {
                        if !is_ascii_control!(c) {
                            self.counter = 0;
                        }
                    } else {
                        self.counter = 0;
                    }
                }
            } else if self.in_js_tag {
                self.buffer.push(c);

                if c == '<' {
                    if self.counter == 0 {
                        self.counter = 1;
                    }
                } else if self.counter == 1 && c == '/' {
                    self.counter = 2;
                } else if self.counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.counter = 3;
                } else if self.counter == 3 && c.to_ascii_lowercase() == 'c' {
                    self.counter = 4;
                } else if self.counter == 4 && c.to_ascii_lowercase() == 'r' {
                    self.counter = 5;
                } else if self.counter == 5 && c.to_ascii_lowercase() == 'i' {
                    self.counter = 6;
                } else if self.counter == 6 && c.to_ascii_lowercase() == 'p' {
                    self.counter = 7;
                } else if self.counter == 7 && c.to_ascii_lowercase() == 't' {
                    self.counter = 8;
                } else if self.counter == 8 && c == '>' {
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
                        } else if !is_ascii_control!(c) {
                            temp.insert(0, c);
                        }

                        e = e - 1;
                    }

                    let minified_js = js::minify(&buffer.iter().collect::<String>());
                    buffer.clear();

                    self.out.extend(minified_js.chars());

                    self.out.extend(temp);

                    self.is_just_finish_tagging = true;
                } else {
                    if self.counter == 1 || self.counter == 2 || self.counter == 8 {
                        if !is_ascii_control!(c) {
                            self.counter = 0;
                        }
                    } else {
                        self.counter = 0;
                    }
                }

                continue;
            } else if self.in_css_tag {
                self.buffer.push(c);

                if c == '<' {
                    if self.counter == 0 {
                        self.counter = 1;
                    }
                } else if self.counter == 1 && c == '/' {
                    self.counter = 2;
                } else if self.counter == 2 && c.to_ascii_lowercase() == 's' {
                    self.counter = 3;
                } else if self.counter == 3 && c.to_ascii_lowercase() == 't' {
                    self.counter = 4;
                } else if self.counter == 4 && c.to_ascii_lowercase() == 'y' {
                    self.counter = 5;
                } else if self.counter == 5 && c.to_ascii_lowercase() == 'l' {
                    self.counter = 6;
                } else if self.counter == 6 && c.to_ascii_lowercase() == 'e' {
                    self.counter = 7;
                } else if self.counter == 7 && c == '>' {
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
                        } else if !is_ascii_control!(c) {
                            temp.insert(0, c);
                        }

                        e = e - 1;
                    }

                    let minified_css = css::minify(&buffer.iter().collect::<String>())?;
                    buffer.clear();

                    self.out.extend(minified_css.chars());

                    self.out.extend(temp);

                    self.is_just_finish_tagging = true;
                } else {
                    if self.counter == 1 || self.counter == 2 || self.counter == 7 {
                        if !is_ascii_control!(c) {
                            self.counter = 0;
                        }
                    } else {
                        self.counter = 0;
                    }
                }

                continue;
            } else if is_space_or_new_line!(c) {
                if self.ignoring_space {
                    continue;
                } else if self.in_start_tagging {
                    self.in_start_tagging = false;
                } else if self.in_end_tagging {
                    self.in_start_tagging = false;
                } else if self.is_tagging {
                    if self.in_attribute {
                        if self.attribute.eq(&"class".to_string()) {
                            if self.last_space {
                                continue;
                            }
                        }
                    } else {
                        if self.last_space {
                            continue;
                        }
                    }
                } else if self.last_space {
                    continue;
                }

                if c == '\n' {
                    self.line_space = true;
                    self.out.push(' ');
                } else if c == '\t' {
                    self.line_space = false;
                    self.out.push(' ');
                } else {
                    self.line_space = false;
                    self.out.push(c);
                }
                self.last_space = true;
                continue;
            } else {
                if c == '<' && !self.is_tagging {
                    self.is_tagging = true;
                    self.buffer.clear();
                    self.in_starting_tagging = true;
                    self.is_start_tagging = true;
                    self.ignoring_space = true;

                    if self.last_space && self.line_space {
                        let end = self.out.len() - 1;
                        self.out.remove(end);
                    }
                } else if self.in_starting_tagging {
                    self.in_starting_tagging = false;

                    if c == '>' {
                        self.is_tagging = false;
                        self.is_just_finish_tagging = true;
                    } else if c == '/' {
                        self.in_end_tagging = true;
                        self.is_start_tagging = false;
                        self.end_tag.clear();
                    } else {
                        if c == '!' {
                            self.counter = 1;
                        } else {
                            self.counter = 0;
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
                        self.ignoring_space = false;

                        into_tag!(self);
                    } else if c == '/' {
                        self.in_start_tagging = false;
                    } else {
                        if self.counter > 0 {
                            if c == '-' {
                                self.counter += 1;
                            }

                            if self.counter == 3 {
                                self.in_start_tagging = false;
                                self.is_tagging = false;
                                // self.is_just_end_tagging = true;
                                self.is_comment = true;
                                self.counter = 0;

                                let len = self.out.len();

                                for i in ((len - 3)..len).rev() {
                                    self.out.remove(i);
                                }

                                if self.last_space {
                                    if !is_space_or_new_line!(*self.out.last().unwrap()) {
                                        self.last_space = false;
                                    }
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
                        self.ignoring_space = false;
                    } else {
                        self.end_tag.push(c);
                    }
                } else if self.is_tagging {
                    if self.in_attribute {
                        if c == self.attribute_quote && *self.out.last().unwrap() != '\\' {
                            self.in_attribute = false;
                        }
                    } else {
                        if c == '>' {
                            if self.last_space {
                                let end = self.out.len() - 1;
                                self.out.remove(end);
                            }

                            self.is_tagging = false;
                            self.is_just_finish_tagging = true;
                            self.ignoring_space = false;

                            if self.is_start_tagging {
                                into_tag!(self);
                            }
                        } else if c == '/' {
                            if self.last_space {
                                let end = self.out.len() - 1;
                                self.out.remove(end);
                            }
                        } else if c == '"' || c == '\'' {
                            self.in_attribute = true;
                            self.attribute_quote = c;
                        } else if c == '=' {
                            self.attribute = self.buffer.iter().collect::<String>().to_lowercase();
                            self.buffer.clear();
                        } else {
                            self.buffer.push(c);
                        }
                    }
                } else {
                    if self.is_just_finish_tagging {
                        if self.last_space && self.line_space {
                            let end = self.out.len() - 1;
                            self.out.remove(end);
                        }
                    }
                    self.ignoring_space = false;
                }

                self.last_space = false;
            }

            self.out.push(c);
        }

        Ok(())
    }

    /// Finalize and generate HTML code into a string instance.
    pub fn get_html(mut self) -> String {
        let mut len = self.out.len();

        loop {
            if len == 0 {
                return "".to_string();
            }

            let c = *self.out.last().unwrap();

            if c != ' ' && c != '\t' && c != '\n' {
                break;
            }

            len = len - 1;
            self.out.remove(len);
        }


        self.out.into_iter().collect()
    }

    /// Minify HTML.
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