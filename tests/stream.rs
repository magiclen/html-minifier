use std::io::{BufWriter, Read, Write};

use html_minifier::{HTMLMinifier, HTMLMinifierError, HTMLMinifierHelper, minify};

fn default_options(_: &mut HTMLMinifier) {}

fn minify_embedded(minifier: &mut HTMLMinifier) {
    minifier.set_minify_js(true);
    minifier.set_minify_css(true);
}

fn preserve_code(minifier: &mut HTMLMinifier) {
    minifier.set_minify_code(false);
}

fn keep_comments(minifier: &mut HTMLMinifier) {
    minifier.set_remove_comments(false);
}

fn test_cases(cases: &[(&str, &str)], configure: fn(&mut HTMLMinifier)) {
    for &(expected, input) in cases {
        let bytes = input.as_bytes();

        for split in 0..=bytes.len() {
            let mut minifier = HTMLMinifier::new();
            configure(&mut minifier);
            minifier.digest(&bytes[..split]).unwrap();
            let _ = format!("{minifier:?}");
            minifier.digest(&bytes[split..]).unwrap();
            minifier.finish().unwrap();
            assert_eq!(expected.as_bytes(), minifier.get_html(), "split {split}: {input:?}");
        }

        let mut minifier = HTMLMinifier::new();
        configure(&mut minifier);
        for byte in bytes {
            minifier.digest([*byte]).unwrap();
            let _ = format!("{minifier:?}");
        }
        assert_eq!(expected.as_bytes(), minifier.into_html().unwrap(), "bytes: {input:?}");
    }
}

#[test]
fn unquoted_paths_and_special_elements() {
    test_cases(
        &[
            ("<a href=/foo>", "<a href=/foo>"),
            ("<img src=//example.test/a>", "<img src=//example.test/a>"),
            ("<input value=x />", "<input value=x  />"),
            ("<textarea rows=2>a  b</textarea>", "<textarea rows=2>a  b</textarea>"),
            ("<pre class=x>a  b</pre>", "<pre class=x>a  b</pre>"),
            (
                "<script type=module>let x = 'a  b';</script>",
                "<script type=module>let x = 'a  b';</script>",
            ),
            (
                "<style media=screen>.a { color: red; }</style>",
                "<style media=screen>.a { color: red; }</style>",
            ),
        ],
        default_options,
    );
    test_cases(&[("<code class=x>a  b</code>", "<code class=x>a  b</code>")], preserve_code);
    test_cases(
        &[
            ("<script type=module>let x=1;</script>", "<script type=module> let x = 1; </script>"),
            (
                "<style media=screen>.a{color:red;}</style>",
                "<style media=screen> .a { color: red; } </style>",
            ),
        ],
        minify_embedded,
    );
}

#[test]
fn unicode_class_spacing() {
    test_cases(
        &[
            ("<div class='a 中文'>", "<div class='  a   中文  '>"),
            ("<div class='中文 a'>", "<div class='中文   a'>"),
            ("<div class='中文 字 😀'>", "<div class='中文  字  😀'>"),
            ("<div title='a   中文'>", "<div title='a   中文'>"),
        ],
        default_options,
    );
}

#[test]
fn embedded_content_is_preserved_by_default() {
    test_cases(
        &[
            (
                "<script>console.log(1)\nconsole.log(2)</script>",
                "<script>console.log(1)\nconsole.log(2)</script>",
            ),
            (
                "<style>.a{width:min(100% + 1px,50px)}</style>",
                "<style>.a{width:min(100% + 1px,50px)}</style>",
            ),
            ("<script>const x = `中文  😀`;</script>", "<script>const x = `中文  😀`;</script>"),
        ],
        default_options,
    );
    assert_eq!("<script> a(); </script>", minify("<script> a(); </script>").unwrap());
}

#[test]
fn embedded_options_are_independent() {
    let input = "<script> a(); </script><style> p { color: red; } </style>";
    let mut minifier = HTMLMinifier::new();
    assert!(!minifier.get_minify_js());
    assert!(!minifier.get_minify_css());
    minifier.set_minify_js(true);
    minifier.digest(input).unwrap();
    assert_eq!(
        b"<script>a();</script><style> p { color: red; } </style>",
        minifier.into_html().unwrap().as_slice()
    );

    let mut minifier = HTMLMinifier::new();
    minifier.set_minify_css(true);
    minifier.digest(input).unwrap();
    assert_eq!(
        b"<script> a(); </script><style>p{color:red;}</style>",
        minifier.into_html().unwrap().as_slice()
    );
}

#[test]
fn crlf_does_not_reorder_embedded_content() {
    test_cases(
        &[
            ("<script>a();b();</script>", "<script>a();b();\r\n</script>"),
            ("<style>p{color:red;}</style>", "<style>p {\r\n color: red;\r\n}</style>"),
            ("<script>alert('中文');</script>", "<script>\r\n alert('中文');\r\n</script>"),
        ],
        minify_embedded,
    );
}

#[test]
fn html_whitespace_keeps_tokens_separate() {
    test_cases(
        &[
            ("<p>a b</p>", "<p>a\rb</p>"),
            ("<input type=text>", "<input\rtype=text>"),
            ("<input type=text>", "<input\x0Ctype=text>"),
            ("<div class='a b'>", "<div class='a\x0Cb'>"),
            ("<pre>a\nb\nc</pre>", "<pre>a\rb\r\nc</pre>"),
            ("<textarea>a\nb</textarea>", "<textarea>a\r\nb</textarea>"),
            ("<p title='a\nb'>", "<p title='a\r\nb'>"),
        ],
        default_options,
    );
}

#[test]
fn comments_end_like_browsers() {
    test_cases(
        &[
            ("abc", "a<!-- x --->b<!-- y -->c"),
            ("abc", "a<!-- x ---->b<!-- y -->c"),
            ("abc", "a<!-->b<!--->c"),
        ],
        default_options,
    );
    test_cases(
        &[
            ("a <!-- x --> b", "a  <!-- x -->  b"),
            ("<p>a</p> <!-- x --> <p>b</p>", "<p>a</p>  <!-- x -->  <p>b</p>"),
            ("a\n<!-- x -->\nb", "a\n<!-- x -->\nb"),
            ("a<!-- x --->b", "a<!-- x --->b"),
        ],
        keep_comments,
    );
}

#[test]
fn preserved_html_tracks_quotes_comments_and_raw_children() {
    test_cases(
        &[
            (
                "<pre><span title=\"</pre>\">a   b</span></pre>",
                "<pre><span title=\"</pre>\">a   b</span></pre>",
            ),
            ("<pre><!-- </pre> -->a   b</pre>", "<pre><!-- </pre> -->a   b</pre>"),
            (
                "<pre><pre-example>a   b</pre-example>c   d</pre>",
                "<pre><pre-example>a   b</pre-example>c   d</pre>",
            ),
            (
                "<pre><script>let x = '</pre>';</script>a   b</pre>",
                "<pre><script>let x = '</pre>';</script>a   b</pre>",
            ),
            (
                "<pre><script/>let x = '</pre>';</script>a   b</pre>",
                "<pre><script/>let x = '</pre>';</script>a   b</pre>",
            ),
            (
                "<pre><textarea></pre></textarea>a   b</pre>",
                "<pre><textarea></pre></textarea>a   b</pre>",
            ),
            ("<pre><title></pre></title>a   b</pre>", "<pre><title></pre></title>a   b</pre>"),
            (
                "<pre><style>/* </pre> */</style>a   b</pre>",
                "<pre><style>/* </pre> */</style>a   b</pre>",
            ),
            ("<PRE>a   b</PrE><p>c d</p>", "<PRE>a   b</PrE  ><p>c   d</p>"),
            ("<pre><img alt=Don't></pre><p>a b</p>", "<pre><img alt=Don't></pre><p>a   b</p>"),
            ("<pre><![CDATA[it's]]></pre><p>a b</p>", "<pre><![CDATA[it's]]></pre><p>a   b</p>"),
            ("<pre><!-->x</pre><p>a b</p>", "<pre><!-->x</pre><p>a   b</p>"),
        ],
        default_options,
    );
    test_cases(
        &[
            (
                "<code>a <code>b</code>  c</code><p>d e</p>",
                "<code>a <code>b</code>  c</code><p>d   e</p>",
            ),
            (
                "<code><span title='</code>'>a   b</span></code>",
                "<code><span title='</code>'>a   b</span></code>",
            ),
            ("<code><!-- </code> -->a   b</code>", "<code><!-- </code> -->a   b</code>"),
            (
                "<code><code-example>a   b</code-example>c   d</code>",
                "<code><code-example>a   b</code-example>c   d</code>",
            ),
            (
                "<code><!-- <code> --><code>b</code>  c</code><p>d e</p>",
                "<code><!-- <code> --><code>b</code>  c</code><p>d   e</p>",
            ),
            (
                "<code><code/>b</code>  c</code><p>d e</p>",
                "<code><code/>b</code>  c</code><p>d   e</p>",
            ),
        ],
        preserve_code,
    );
}

#[test]
fn closing_tag_overlap_restarts_at_less_than() {
    test_cases(
        &[
            ("<textarea><</textarea><p>a b</p>", "<textarea><</textarea><p>a   b</p>"),
            ("<script>//<</script><p>a b</p>", "<script>//<</script><p>a   b</p>"),
            ("<pre><</pre><p>a b</p>", "<pre><</pre><p>a   b</p>"),
            ("<textarea></text<</textarea><p>a b</p>", "<textarea></text<</textarea><p>a   b</p>"),
        ],
        default_options,
    );
    test_cases(
        &[("<script></script><p>a b</p>", "<script>//<</script><p>a   b</p>")],
        minify_embedded,
    );
}

#[test]
fn embedded_end_tag_prefixes_are_replayed() {
    test_cases(
        &[
            (
                "<script>s=\"</scr\"+\"ipt>\";</script>",
                "<script> s = \"</scr\" + \"ipt>\"; </script>",
            ),
            ("<style>a{content:\"</sty\";}</style>", "<style> a { content: \"</sty\"; } </style>"),
            ("<script>a();</SCRIPT><p>b c</p>", "<script> a(); </SCRIPT  ><p>b   c</p>"),
        ],
        minify_embedded,
    );
}

#[test]
fn attribute_flags_do_not_leak() {
    test_cases(
        &[
            (
                "<div class=x></div><input type='  text  '>",
                "<div class=x></div><input type='  text  '>",
            ),
            (
                "<script type='module'></script><input type='text'><script>a();</script>",
                "<script type='module'></script><input type='text'><script> a(); </script>",
            ),
        ],
        minify_embedded,
    );
}

#[test]
fn embedded_type_detection() {
    test_cases(
        &[
            (
                "<script type='text/javascript'>a();</script>",
                "<script type='text/javascript'> a(); </script>",
            ),
            (
                "<style type='Text/CSS'>p{color:red;}</style>",
                "<style type='Text/CSS'> p { color: red; } </style>",
            ),
            (
                "<style type='text&#47;css'>p{color:red;}</style>",
                "<style type='text&#47;css'> p { color: red; } </style>",
            ),
            (
                "<script type='text/plain' type> a(); </script>",
                "<script type='text/plain' type=''> a(); </script>",
            ),
            (
                "<script type type='text/plain'>a();</script>",
                "<script type='' type='text/plain'> a(); </script>",
            ),
        ],
        minify_embedded,
    );
}

#[test]
fn byte_input_debug_and_utf8_conversion() {
    let mut minifier = HTMLMinifier::new();
    minifier.digest([0xFF]).unwrap();
    let _ = format!("{minifier:?}");
    assert_eq!([0xFF], minifier.into_html().unwrap().as_slice());

    for input in [b"<script>\xFF</script>".as_slice(), b"<style>\xFF</style>"] {
        let mut minifier = HTMLMinifier::new();
        minify_embedded(&mut minifier);
        assert!(matches!(minifier.digest(input), Err(HTMLMinifierError::UTF8Error(_))));
    }

    let mut minifier = HTMLMinifier::new();
    minify_embedded(&mut minifier);
    minifier.digest(b"<script type='\xFF'> a(); </script>").unwrap();
    assert_eq!(b"<script type='\xFF'> a(); </script>", minifier.into_html().unwrap().as_slice());
    test_cases(
        &[(
            "<script type='application/javascript'>alert('中文😀');</script>",
            "<script type='application/javascript'> alert('中文😀'); </script>",
        )],
        minify_embedded,
    );
}

#[test]
fn finish_flushes_literal_prefixes_and_reports_incomplete_input() {
    test_cases(&[("a <", "a <"), ("a </", "a </"), ("a", "a   ")], default_options);
    assert_eq!("a <", minify("a <").unwrap());

    for input in
        ["<div", "<div title='x", "<!-- x", "<script>x", "<style>p {}", "<textarea>x", "<pre>x"]
    {
        let mut minifier = HTMLMinifier::new();
        minifier.digest(input).unwrap();
        assert!(matches!(minifier.finish(), Err(HTMLMinifierError::IncompleteInput)), "{input:?}");
        assert!(matches!(minify(input), Err(HTMLMinifierError::IncompleteInput)), "{input:?}");
    }

    let mut minifier = HTMLMinifier::new();
    minifier.digest("<div>").unwrap();
    minifier.finish().unwrap();
    minifier.finish().unwrap();
    assert_eq!(b"<div>", minifier.get_html());
}

#[test]
fn reset_discards_pending_parser_state_and_preserves_options() {
    for input in ["<div class='x ", "<script type='module'>x", "<pre><span title='", "<!-- x"] {
        let mut minifier = HTMLMinifier::new();
        minify_embedded(&mut minifier);
        minifier.set_minify_code(false);
        minifier.set_remove_comments(false);
        minifier.digest(input).unwrap();
        minifier.reset();
        assert!(minifier.get_minify_js());
        assert!(minifier.get_minify_css());
        assert!(!minifier.get_minify_code());
        assert!(!minifier.get_remove_comments());
        minifier.digest("<input type='  text  '><script> a(); </script><code>a  b</code>").unwrap();
        assert_eq!(
            b"<input type='  text  '><script>a();</script><code>a  b</code>",
            minifier.into_html().unwrap().as_slice()
        );
    }
}

#[test]
fn helper_accepts_reader_chunks_and_buffered_output() {
    let mut input = "<p> 中文  😀 </p><script> a(); </script> <".as_bytes();
    let mut buffer = [0; 2];
    let mut output = BufWriter::new(Vec::new());
    let mut helper = HTMLMinifierHelper::new();

    loop {
        let length = input.read(&mut buffer).unwrap();
        if length == 0 {
            break;
        }
        helper.digest(&buffer[..length], &mut output).unwrap();
        let _ = format!("{helper:?}");
    }
    helper.finish(&mut output).unwrap();
    output.flush().unwrap();
    assert_eq!(
        "<p> 中文 😀 </p><script> a(); </script> <".as_bytes(),
        output.into_inner().unwrap()
    );
}
