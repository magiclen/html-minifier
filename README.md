HTML Minifier
====================

[![Build Status](https://travis-ci.org/magiclen/html-minifier.svg?branch=master)](https://travis-ci.org/magiclen/html-minifier)

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



## Crates.io

https://crates.io/crates/html-minifier

## Documentation

https://docs.rs/html-minifier

## License

[MIT](LICENSE)