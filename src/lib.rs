/*!
# HTML Minifier

This library can help you generate and minify your HTML code at the same time. It also supports to minify JS and CSS in `<style>`, `<script>` elements, and ignores the minification of `<pre>`, `<code>` and `<textarea>` elements.

HTML is minified by the following rules:

* ASCII control characters (0x00-0x08, 0x11-0x1F, 0x7F) are always removed.
* Comments can be optionally removed. (removed by default)
* **Useless** whitespaces (spaces, tabs and newlines) are removed.
* Whitespaces (spaces, tabs and newlines) are converted to a single `'\x20'` or a single '\n', if possible.
* Empty attribute values are collapsed. (e.g `<input readonly="">` => `<input readonly>` )
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
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<!DOCTYPE html>   <html  ").unwrap();
html_minifier.digest("lang=  en >").unwrap();
html_minifier.digest("
<head>
    <meta name=viewport>
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

assert_eq!("<!DOCTYPE html> <html lang=en>
<head>
<meta name=viewport>
</head>
<body class='container bg-light'>
<input type='text' value='123   456' readonly/>
123456
<b>big</b> 789
ab
c
中文
字
</body>
</html>".as_bytes(), html_minifier.get_html());
```

```rust
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<pre  >   Hello  world!   </pre  >").unwrap();

assert_eq!(b"<pre>   Hello  world!   </pre>", html_minifier.get_html());
```

```rust
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();

html_minifier.digest("<script type='  application/javascript '>   alert('Hello!')    ;   </script>").unwrap();

assert_eq!("<script type='application/javascript'>alert('Hello!')</script>".as_bytes(), html_minifier.get_html());
```

## Write HTML to a Writer

If you don't want to store your HTML in memory (e.g. writing to a file instead), you can use the `HTMLMinifierHelper` struct which provides a low-level API that allows you to pass your output instance when invoking the `digest` method.

```rust
use html_minifier::HTMLMinifierHelper;

# #[cfg(feature = "std")] {
use std::fs::File;
use std::io::Read;

let mut input_file = File::open("tests/data/w3schools.com_tryhow_css_example_website.htm").unwrap();
let mut output_file = File::create("tests/data/index.min.html").unwrap();

let mut buffer = [0u8; 256];

let mut html_minifier_helper = HTMLMinifierHelper::new();

loop {
    let c = input_file.read(&mut buffer).unwrap();

    if c == 0 {
        break;
    }

    html_minifier_helper.digest(&buffer[..c], &mut output_file).unwrap();
}
# }
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

mod errors;
mod functions;
mod html_minifier_helper;
mod html_writer;

use alloc::{string::String, vec::Vec};

pub use errors::*;
pub use html_minifier_helper::*;
pub use html_writer::*;

use crate::functions::*;

/// This struct helps you generate and minify your HTML code in the same time. The output destination is inside this struct.
#[derive(Educe, Clone)]
#[educe(Debug, Default(new))]
pub struct HTMLMinifier {
    helper: HTMLMinifierHelper,
    #[educe(Debug(method = "str_bytes_fmt"))]
    out:    Vec<u8>,
}

impl HTMLMinifier {
    /// Set whether to remove HTML comments.
    #[inline]
    pub fn set_remove_comments(&mut self, remove_comments: bool) {
        self.helper.remove_comments = remove_comments;
    }

    /// Set whether to minify the content in the `code` element.
    #[inline]
    pub fn set_minify_code(&mut self, minify_code: bool) {
        self.helper.minify_code = minify_code;
    }

    /// Get whether to remove HTML comments.
    #[inline]
    pub fn get_remove_comments(&self) -> bool {
        self.helper.remove_comments
    }

    /// Get whether to minify the content in the `code` element.
    #[inline]
    pub fn get_minify_code(&self) -> bool {
        self.helper.minify_code
    }
}

impl HTMLMinifier {
    /// Reset this html minifier. The option settings and allocated memory will be be preserved.
    #[inline]
    pub fn reset(&mut self) {
        self.helper.reset();
        self.out.clear();
    }
}

impl HTMLMinifier {
    /// Input some text to generate HTML code. It is not necessary to input a full HTML text at once.
    #[inline]
    pub fn digest<S: AsRef<[u8]>>(&mut self, text: S) -> Result<(), HTMLMinifierError> {
        let text = text.as_ref();

        self.out.reserve(text.len());

        self.helper.digest(text, &mut self.out)
    }

    /// Directly input some text to generate HTML code. The text will just be appended to the output buffer instead of being through the helper.
    ///
    /// # When to Use This?
    ///
    /// If the text has been minified, you can consider to use this method to get a better performance.
    #[allow(clippy::missing_safety_doc)]
    #[inline]
    pub unsafe fn indigest<S: AsRef<[u8]>>(&mut self, text: S) {
        self.out.extend_from_slice(text.as_ref());
    }
}

impl HTMLMinifier {
    /// Get HTML in a string slice.
    #[inline]
    pub fn get_html(&mut self) -> &[u8] {
        self.out.as_slice()
    }
}

/// Minify HTML.
#[inline]
pub fn minify<S: AsRef<str>>(html: S) -> Result<String, HTMLMinifierError> {
    let mut minifier = HTMLMinifierHelper::new();

    let html = html.as_ref();

    let mut minified_html = String::with_capacity(html.len());

    minifier.digest(html, unsafe { minified_html.as_mut_vec() })?;

    Ok(minified_html)
}
