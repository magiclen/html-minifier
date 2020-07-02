HTML Minifier
====================

[![Build Status](https://travis-ci.org/magiclen/html-minifier.svg?branch=master)](https://travis-ci.org/magiclen/html-minifier)

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

## Crates.io

https://crates.io/crates/html-minifier

## Documentation

https://docs.rs/html-minifier

## License

[MIT](LICENSE)