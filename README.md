HTML Minifier
====================

[![Build Status](https://travis-ci.org/magiclen/html-minifier.svg?branch=master)](https://travis-ci.org/magiclen/html-minifier)

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

## Crates.io

https://crates.io/crates/html-minifier

## Documentation

https://docs.rs/html-minifier

## License

[MIT](LICENSE)