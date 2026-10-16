HTML Minifier
====================

[![CI](https://github.com/magiclen/html-minifier/actions/workflows/ci.yml/badge.svg)](https://github.com/magiclen/html-minifier/actions/workflows/ci.yml)

This library minifies HTML as you generate or read it. It preserves the content of `<pre>` and `<textarea>` elements and can also preserve `<code>` content. Embedded JavaScript and CSS are also minified by removing comments and whitespace.

HTML is minified by the following rules:

* ASCII control characters (0x00-0x08, 0x0B, 0x0E-0x1F, 0x7F) are always removed.
* Comments can be optionally removed. (removed by default)
  Empty comments are kept when needed to keep text from forming a different character reference or tag.
* Unnecessary whitespace is removed.
* HTML whitespace (spaces, tabs, line feeds, carriage returns and form feeds) is collapsed to a single space or line feed where applicable.
* Empty attribute values are collapsed. (e.g `<input readonly="">` => `<input readonly>` )
* The inner HTML of all elements is minified except for the following elements:
  * `<pre>` and `<listing>`
  * `<textarea>`
  * `<title>` (only whitespace is collapsed)
  * `<xmp>`, `<iframe>`, `<noembed>` and `<noframes>`
  * `<svg>` and `<math>` (including their start tags and HTML children)
  * `<plaintext>` (all content after its start tag is preserved until the end of input)
  * `<code>` (optionally, minified by default)
  * `<style>` (preserved if CSS minification is disabled or its `type` is unsupported)
  * `<script>` (preserved if JavaScript minification is disabled or its `type` is unsupported)
* Embedded JavaScript and CSS are minified by removing comments and whitespace.

The original (non-minified) HTML doesn't need to be completely generated before using this library because this library doesn't do any deserialization to create DOMs.

`digest` accepts byte slices, including chunks that split a UTF-8 character. Call `finish` after the last chunk. `get_html` returns the current output snapshot and may omit pending content until `finish` succeeds. `into_html` calls `finish` and returns the owned output buffer without copying it. The `minify` convenience function finishes the input automatically.

`finish` reports incomplete tags, comments and special elements. It preserves a trailing literal `<` or `</`. It does not validate the whole document or require every ordinary element to have a closing tag.

Foreign SVG and MathML content is preserved as one region, including comments and embedded code. HTML minification resumes after the region ends. Like browsers, an HTML tag that cannot be in foreign content, such as `<p>` directly in `<svg>`, ends the region. A `plaintext` element consumes all remaining input, including text that looks like an end tag. Preserved content still follows the control-character and line-break rules.

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

html_minifier.digest("<script type='  application/javascript '>   alert('Hello!')    ;   </script>").unwrap();
html_minifier.finish().unwrap();

assert_eq!("<script type='  application/javascript '>alert('Hello!');</script>".as_bytes(), html_minifier.get_html());
```

## Options and Embedded Code

| Setter | Default | Effect |
| --- | --- | --- |
| `set_remove_comments` | `true` | Remove HTML comments outside preserved content, except empty comments needed to keep text from forming a different character reference or tag. |
| `set_minify_code` | `true` | Minify HTML inside `<code>` elements. |
| `set_minify_js` | `true` | Minify supported JavaScript content. |
| `set_minify_css` | `true` | Minify supported CSS content. |

Each option has a corresponding `get_` method. `HTMLMinifierHelper` exposes the same options as public fields. Set options before digesting the document. `reset` clears parsing state and output while keeping options and allocated capacity; the helper leaves its external writer unchanged.

JavaScript and CSS can be disabled independently:

```rust
use html_minifier::HTMLMinifier;

let mut html_minifier = HTMLMinifier::new();
html_minifier.set_minify_js(false);
html_minifier.digest("<style> p { color: red; } </style><script> a(); </script>").unwrap();

let html = html_minifier.into_html().unwrap();
assert_eq!(b"<style>p{color:red;}</style><script> a(); </script>", html.as_slice());
```

JavaScript minification supports an absent or empty `type`, `application/javascript`, `text/javascript`, and `module`.
Leading and trailing ASCII whitespace is allowed only around the two JavaScript MIME types.
CSS minification supports an absent or empty `type` and `text/css` without added whitespace.
Type matching ignores ASCII case and decodes character references.
The original attribute value is preserved.
Other types keep their content unchanged.

Removing a comment or `</>` must not join text into a different character reference or tag.
For example, `&co<!-- note -->py;` becomes `&co<!---->py;` so its text stays `&copy;` instead of becoming `©`.
Likewise, `a<<!-- note -->b` becomes `a<<!---->b` so `<b` does not start a tag.

JavaScript and CSS minification only removes comments and whitespace, so the code keeps its meaning. JavaScript line breaks that automatic semicolon insertion (ASI) needs are kept. When the meaning of a slash is unclear, the rest of that script is preserved. Block comments whose text starts with `!` or contains `@lic` (such as `@license`) or `@preserve` are kept. Preserved content still follows the control-character removal rule above.

## Errors

`HTMLMinifierError` is non-exhaustive and includes `IncompleteInput` and `IOError`. Byte input is preserved without requiring each chunk to be valid UTF-8. Debug formatting also supports incomplete or invalid UTF-8.

`IOError` shows the message of the underlying I/O error, and `source()` returns the source of that error. An error can leave partial output; reset the parser and use a fresh output destination before retrying a document.

## Write HTML to a Writer

Use `HTMLMinifierHelper` to supply an output writer. Wrap a file in `BufWriter` to combine small writes.

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

## Crates.io

https://crates.io/crates/html-minifier

## Documentation

https://docs.rs/html-minifier

## License

[MIT](LICENSE)
