/*!
# HTML Minifier

This library minifies HTML as you generate or read it. It preserves the content of `<pre>` and `<textarea>` elements and can also preserve `<code>` content. JavaScript and CSS minification is optional and disabled by default.

HTML is minified by the following rules:

* ASCII control characters (0x00-0x08, 0x0B, 0x0E-0x1F, 0x7F) are always removed.
* Comments can be optionally removed. (removed by default)
* Unnecessary whitespace is removed.
* HTML whitespace (spaces, tabs, line feeds, carriage returns and form feeds) is collapsed to a single space or line feed where applicable.
* Empty attribute values are collapsed. (e.g `<input readonly="">` => `<input readonly>` )
* The inner HTML of all elements is minified except for the following elements:
  * `<pre>`
  * `<textarea>`
  * `<code>` (optionally, minified by default)
  * `<style>` (preserved by default, and always preserved if its `type` is unsupported)
  * `<script>` (preserved by default, and always preserved if its `type` is unsupported)
* Optional JavaScript and CSS minification uses [minifier](https://crates.io/crates/minifier).

The original (non-minified) HTML doesn't need to be completely generated before using this library because this library doesn't do any deserialization to create DOMs.

`digest` accepts byte slices, including chunks that split a UTF-8 character. Call `finish` after the last chunk. `get_html` returns the current output snapshot and may omit pending content until `finish` succeeds. `into_html` calls `finish` and returns the owned output buffer without copying it. The `minify` convenience function finishes the input automatically.

`finish` reports incomplete tags, comments and special elements. It preserves a trailing literal `<` or `</`. It does not validate the whole document or require every ordinary element to have a closing tag.

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
html_minifier.finish().unwrap();

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
html_minifier.finish().unwrap();

assert_eq!(b"<pre>   Hello  world!   </pre>", html_minifier.get_html());
```

```rust
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();
html_minifier.set_minify_js(true);

html_minifier.digest("<script type='  application/javascript '>   alert('Hello!')    ;   </script>").unwrap();
html_minifier.finish().unwrap();

assert_eq!("<script type='application/javascript'>alert('Hello!');</script>".as_bytes(), html_minifier.get_html());
```

## Options and Embedded Code

| Setter | Default | Effect |
| --- | --- | --- |
| `set_remove_comments` | `true` | Remove HTML comments outside preserved content. |
| `set_minify_code` | `true` | Minify HTML inside `<code>` elements. |
| `set_minify_js` | `false` | Minify supported JavaScript content. |
| `set_minify_css` | `false` | Minify supported CSS content. |

Each option has a corresponding `get_` method. `HTMLMinifierHelper` exposes the same options as public fields. Set options before digesting the document. `reset` clears parsing state and output while keeping options and allocated capacity; the helper leaves its external writer unchanged.

JavaScript and CSS can be enabled independently:

```rust
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();
html_minifier.set_minify_css(true);
html_minifier.digest("<style> p { color: red; } </style><script> a(); </script>").unwrap();

let html = html_minifier.into_html().unwrap();
assert_eq!(b"<style>p{color:red;}</style><script> a(); </script>", html.as_slice());
```

JavaScript minification supports an absent or empty `type`, `application/javascript`, `text/javascript`, and `module`. CSS minification supports an absent or empty `type` and `text/css`. Other types remain preserved.

The embedded minifier has syntax limitations. JavaScript automatic semicolon insertion (ASI) can be changed by removing newlines. CSS math outside `calc`, including `min`, `max`, `clamp` and custom property expressions, can lose required spaces around `+`. Leave the relevant option disabled for such content. Preserved content still follows the control-character removal rule above.

The public `html_minifier::js` and `html_minifier::css` modules re-export `minifier` 0.4. Both `js::minify` and `css::minify` return `Result`; callers using these modules must handle errors.

## Errors

`HTMLMinifierError` includes `UTF8Error`, `IncompleteInput`, `JSError`, `CSSError`, and `IOError`. UTF-8 is checked when embedded content must be converted to text. Byte input is otherwise preserved without requiring each chunk to be valid UTF-8. Debug formatting also supports incomplete or invalid UTF-8.

The `source()` method exposes the underlying UTF-8 or I/O error. An error can leave partial output; reset the parser and use a fresh output destination before retrying a document.

## Write HTML to a Writer

Use `HTMLMinifierHelper` to supply an output writer. Wrap a file in `BufWriter` to combine small writes. Optional JavaScript and CSS minification still buffers each embedded block until its closing tag.

```rust,no_run
use html_minifier::HTMLMinifierHelper;

use std::fs::File;
use std::io::{BufWriter, Read, Write};

let mut input_file = File::open("tests/data/w3schools.com_tryhow_css_example_website.htm").unwrap();
let output_file = File::create("index.min.html").unwrap();
let mut output = BufWriter::new(output_file);

let mut buffer = [0u8; 256];

let mut html_minifier_helper = HTMLMinifierHelper::new();

loop {
    let c = input_file.read(&mut buffer).unwrap();

    if c == 0 {
        break;
    }

    html_minifier_helper.digest(&buffer[..c], &mut output).unwrap();
}

html_minifier_helper.finish(&mut output).unwrap();
output.flush().unwrap();
```
*/
mod errors;
mod functions;
mod html_minifier_helper;
mod html_writer;

use educe::Educe;
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

    /// Set whether to minify JavaScript in supported script elements.
    #[inline]
    pub fn set_minify_js(&mut self, minify_js: bool) {
        self.helper.minify_js = minify_js;
    }

    /// Set whether to minify CSS in supported style elements.
    #[inline]
    pub fn set_minify_css(&mut self, minify_css: bool) {
        self.helper.minify_css = minify_css;
    }

    /// Get whether to remove HTML comments.
    #[inline]
    pub const fn get_remove_comments(&self) -> bool {
        self.helper.remove_comments
    }

    /// Get whether to minify the content in the `code` element.
    #[inline]
    pub const fn get_minify_code(&self) -> bool {
        self.helper.minify_code
    }

    /// Get whether to minify JavaScript in supported script elements.
    #[inline]
    pub const fn get_minify_js(&self) -> bool {
        self.helper.minify_js
    }

    /// Get whether to minify CSS in supported style elements.
    #[inline]
    pub const fn get_minify_css(&self) -> bool {
        self.helper.minify_css
    }
}

impl HTMLMinifier {
    /// Reset this HTML minifier while keeping its options and allocated memory.
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

    /// Finish the current input and report any incomplete tag or special element.
    #[inline]
    pub fn finish(&mut self) -> Result<(), HTMLMinifierError> {
        self.helper.finish(&mut self.out)
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
    /// Get the current output bytes, which may be incomplete until `finish` succeeds.
    #[inline]
    pub fn get_html(&self) -> &[u8] {
        self.out.as_slice()
    }

    /// Finish the input and return its output buffer without copying it.
    #[inline]
    pub fn into_html(mut self) -> Result<Vec<u8>, HTMLMinifierError> {
        self.finish()?;
        Ok(self.out)
    }
}

/// Minify HTML.
#[inline]
pub fn minify<S: AsRef<str>>(html: S) -> Result<String, HTMLMinifierError> {
    let mut minifier = HTMLMinifierHelper::new();

    let html = html.as_ref();

    let mut minified_html = String::with_capacity(html.len());

    // The input is UTF-8, and the helper changes only ASCII bytes or adds valid text.
    let out = unsafe { minified_html.as_mut_vec() };
    minifier.digest(html, out)?;
    minifier.finish(out)?;

    Ok(minified_html)
}
