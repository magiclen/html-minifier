use std::io::{BufWriter, Read, Write};

use html_minifier::{HTMLMinifier, HTMLMinifierError, HTMLMinifierHelper, minify};

fn default_options(_: &mut HTMLMinifier) {}

fn minify_embedded(minifier: &mut HTMLMinifier) {
    minifier.set_minify_js(true);
    minifier.set_minify_css(true);
}

fn preserve_embedded(minifier: &mut HTMLMinifier) {
    minifier.set_minify_js(false);
    minifier.set_minify_css(false);
}

fn preserve_code(minifier: &mut HTMLMinifier) {
    minifier.set_minify_code(false);
}

fn keep_comments(minifier: &mut HTMLMinifier) {
    minifier.set_remove_comments(false);
}

/// Count the sequences that change how browsers find the end of a `script` or `style` element.
fn count_html_sequences(html: &str) -> Vec<usize> {
    let html = html.to_ascii_lowercase();

    ["<!--", "-->", "<script", "</script", "</style"]
        .iter()
        .map(|sequence| html.matches(sequence).count())
        .collect()
}

fn test_cases(cases: &[(&str, &str)], configure: fn(&mut HTMLMinifier)) {
    for &(expected, input) in cases {
        let bytes = input.as_bytes();

        for (input_count, output_count) in
            count_html_sequences(input).into_iter().zip(count_html_sequences(expected))
        {
            assert!(output_count <= input_count, "new HTML sequence: {input:?}");
        }

        let mut minifier = HTMLMinifier::new();
        configure(&mut minifier);
        minifier.digest(expected).unwrap();
        assert_eq!(expected.as_bytes(), minifier.into_html().unwrap(), "again: {input:?}");

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
        preserve_embedded,
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
fn embedded_content_is_minified_by_default() {
    test_cases(
        &[
            (
                "<script>console.log(1)\nconsole.log(2)</script>",
                "<script>\n  console.log(1)\n  console.log(2)\n</script>",
            ),
            (
                "<style>.a{width:min(100% + 1px,50px)}</style>",
                "<style>.a { width: min(100% + 1px, 50px) }</style>",
            ),
            ("<script>const x=`中文  😀`;</script>", "<script>const x = `中文  😀`;</script>"),
        ],
        default_options,
    );
    assert_eq!("<script>a();</script>", minify("<script> a(); </script>").unwrap());
}

#[test]
fn embedded_options_are_independent() {
    let input = "<script> a(); </script><style> p { color: red; } </style>";
    let mut minifier = HTMLMinifier::new();
    assert!(minifier.get_minify_js());
    assert!(minifier.get_minify_css());
    minifier.set_minify_css(false);
    minifier.digest(input).unwrap();
    assert_eq!(
        b"<script>a();</script><style> p { color: red; } </style>",
        minifier.into_html().unwrap().as_slice()
    );

    let mut minifier = HTMLMinifier::new();
    minifier.set_minify_js(false);
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
            ("ab", "a<!-- x --!>b"),
            ("<!DOCTYPE html>", "<!-- license -->\n<!DOCTYPE html>"),
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
fn removed_comments_keep_character_reference_boundaries() {
    test_cases(
        &[
            ("<p>&co<!---->py;</p>", "<p>&co<!-- note -->py;</p>"),
            ("<p>&copy<!---->;</p>", "<p>&copy<!-- note -->;</p>"),
            ("<p>&#65<!---->0;</p>", "<p>&#65<!-- note -->0;</p>"),
            ("<p>&#x26<!---->0;</p>", "<p>&#x26<!-- note -->0;</p>"),
            ("<p>&co<!---->py;</p>", "<p>&co<!-- first --><!-- second -->py;</p>"),
            ("<p>&co甲py;</p>", "<p>&co甲<!-- note -->py;</p>"),
            ("<p>a<<!---->b</p>", "<p>a<<!-- note -->b</p>"),
            ("<p>&co</>py;</p>", "<p>&co</>py;</p>"),
            (
                "<p>&copy; &amp; &#65; &#x26;</p>",
                "<p>&copy;<!-- a --> &amp;<!-- b --> &#65;<!-- c --> &#x26;<!-- d --></p>",
            ),
        ],
        default_options,
    );
}

#[test]
fn removed_comment_chains_keep_one_whitespace() {
    test_cases(
        &[
            ("<p>a b</p>", "<p>a <!-- first --> <!-- second --> b</p>"),
            ("<p>a b</p>", "<p>a <!-- first --><!-- second --> <!-- third --> b</p>"),
            ("<p>a\nb</p>", "<p>a\n<!-- first -->\n<!-- second -->\n b</p>"),
            ("<p>a b</p>", "<p>a <!-- first -->\n<!-- second --> b</p>"),
            ("<p>甲 乙</p>", "<p>甲 <!-- first --> <!-- second --> 乙</p>"),
            ("<p>a b</p>", "<p>a<!-- first --> <!-- second --> b</p>"),
            ("<p>ab</p>", "<p>a<!-- first --><!-- second -->b</p>"),
            ("<p>a <b>x</b> b</p>", "<p>a <!-- first --><b>x</b> <!-- second --> b</p>"),
        ],
        default_options,
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
            ("<pre><!-- --!></pre><p>a b</p>", "<pre><!-- --!></pre><p>a   b</p>"),
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
        preserve_embedded,
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
fn javascript_minification_keeps_meaning() {
    test_cases(
        &[
            (
                "<script>window.dataLayer=window.dataLayer||[];function \
                 gtag(){dataLayer.push(arguments);}\ngtag('js',new \
                 Date());gtag('config','G-XXXX');</script>",
                "<script>\n  window.dataLayer = window.dataLayer || [];\n  function \
                 gtag(){dataLayer.push(arguments);}\n  gtag('js', new Date());\n\n  \
                 gtag('config', 'G-XXXX');\n</script>",
            ),
            (
                "<script>console.log(1)\nconsole.log(2)</script>",
                "<script>\nconsole.log(1)\nconsole.log(2)\n</script>",
            ),
            (
                "<script>a=b+ +c- -d;e=1 .toString();f=a++ +b</script>",
                "<script>a = b + +c - -d; e = 1 .toString(); f = a++ + b</script>",
            ),
            (
                "<script>if(a)/x  y/.test(s);z=a/b/c;s='  ';</script>",
                "<script>if (a) /x  y/.test(s); z = a / b / c; s = '  ';</script>",
            ),
            (
                "<script>t=`a ${b+`c ${d}`} e`;</script>",
                "<script>t = `a ${ b + `c ${ d }` } e`;</script>",
            ),
            (
                "<script>/*! keep */\nx=1;y=2\nz=3</script>",
                "<script>/*! keep */\n// drop\nx = 1; /* drop */ y = 2 /* line\n break */ z = \
                 3</script>",
            ),
            ("<script><!--\nalert(1)</script>", "<script><!--\nalert(1)\n//--></script>"),
            (
                "<script><!--\ndocument.write(\"<script>a()</scr\" + \"ipt>\")\n  //-->\n</script>",
                "<script><!--\ndocument.write(\"<script>a()</scr\" + \"ipt>\")\n  //-->\n</script>",
            ),
            (
                "<script>if(a< script)b=a< /x/.test(c)</script>",
                "<script>if (a < script) b = a < /x/.test(c)</script>",
            ),
            (
                "<script type=module>import a from\"a\"\nexport default a</script>",
                "<script type=module>import a from \"a\"\nexport default a</script>",
            ),
            ("<script>x=1\n--> note\ny=2</script>", "<script>x = 1\n--> note\ny = 2</script>"),
            ("<script>/*! a */ /*! b */x()</script>", "<script>/*! a */ /*! b */x()</script>"),
        ],
        minify_embedded,
    );
}

#[test]
fn css_minification_keeps_meaning() {
    test_cases(
        &[
            (
                "<style>.a :hover,.b>.c~.d{color:red;}</style>",
                "<style>\n.a :hover, .b > .c ~ .d {\n  color: red;\n}\n</style>",
            ),
            (
                "<style>a{width:calc(100% - 2 * 1px);height:clamp(1rem,2.5vw + 1rem,3rem)}</style>",
                "<style>a { width: calc( 100% - 2 * 1px ); height: clamp(1rem, 2.5vw + 1rem, \
                 3rem) }</style>",
            ),
            (
                "<style>@media screen and (min-width:100px){.a{background:url(a.png)}}</style>",
                "<style>@media screen and (min-width: 100px) {\n  .a { background: url( a.png) \
                 }\n}</style>",
            ),
            (
                "<style>.\\31 a:after{content:\"a  b\";--x:1px  +  2px ;color:red \
                 !important}</style>",
                "<style>.\\31 a:after { content: \"a  b\"; --x:  1px  +  2px ; color: red \
                 !important }</style>",
            ),
            (
                "<style>/*! keep */ a{b:c}/* @license MIT */ d{e:f}</style>",
                "<style>/*! keep */ a { b: c } /* drop */ /* @license MIT */ d { e: f }</style>",
            ),
            ("<style>.a.b{}a/**/b{}</style>", "<style>.a/**/.b {} a/**/b {}</style>"),
            ("<style>.\\31  .b{}</style>", "<style>.\\31  .b {}</style>"),
            ("<style><!-- a{b:c}--></style>", "<style><!-- a { b: c } --></style>"),
            ("<style>a{--x: ;color:red}</style>", "<style>a { --x:  ; color: red }</style>"),
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
fn embedded_types_keep_their_original_whitespace() {
    test_cases(
        &[
            (
                "<script type='  application/javascript  '>a();</script>",
                "<script type='  application/javascript  '> a(); </script>",
            ),
            (
                "<script type='\tText/JavaScript\n'>a();</script>",
                "<script type='\tText/JavaScript\n'> a(); </script>",
            ),
            (
                "<script type='&#32;text&#47;javascript&#9;'>a();</script>",
                "<script type='&#32;text&#47;javascript&#9;'> a(); </script>",
            ),
            (
                "<script type='mo&#100;ule'>a();</script>",
                "<script type='mo&#100;ule'> a(); </script>",
            ),
            ("<style type>p{color:red;}</style>", "<style type=''> p { color: red; } </style>"),
        ],
        default_options,
    );

    for input in [
        "<script type=' module '> a(); </script>",
        "<script type=' importmap '> { \"imports\": {} } </script>",
        "<script type=' speculationrules '> { \"prefetch\": [] } </script>",
        "<script type='   '> a(); </script>",
        "<script type='&#32;module&#32;'> a(); </script>",
        "<script type='&#32;&#9;'> a(); </script>",
        "<style type=' text/css '> p { color: red; } </style>",
        "<style type='   '> p { color: red; } </style>",
        "<style type='&#32;text&#47;css&#32;'> p { color: red; } </style>",
    ] {
        test_cases(&[(input, input)], default_options);
    }

    for input in [
        "<script type='  application/javascript  '> a(); </script>",
        "<script type=' module '> a(); </script>",
        "<style type=' text/css '> p { color: red; } </style>",
    ] {
        test_cases(&[(input, input)], preserve_embedded);
    }
}

#[test]
fn byte_input_debug_and_utf8_conversion() {
    let mut minifier = HTMLMinifier::new();
    minifier.digest([0xFF]).unwrap();
    let _ = format!("{minifier:?}");
    assert_eq!([0xFF], minifier.into_html().unwrap().as_slice());

    for input in [
        b"<script>a('\xFF');</script>".as_slice(),
        b"<script type='\xFF'> a(); </script>",
        b"<style>a{content:'\xFF'}</style>",
    ] {
        let mut minifier = HTMLMinifier::new();
        minify_embedded(&mut minifier);
        minifier.digest(input).unwrap();
        assert_eq!(input, minifier.into_html().unwrap().as_slice());
    }
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

    for input in ["<div>", "<svg>", "<math>text <", "<svg>text </"] {
        let mut minifier = HTMLMinifier::new();
        minifier.digest(input).unwrap();
        minifier.finish().unwrap();
        minifier.finish().unwrap();
        assert_eq!(input.as_bytes(), minifier.get_html());
    }
}

#[test]
fn reset_discards_pending_parser_state_and_preserves_options() {
    for input in [
        "<div class='x ",
        "<script type='module'>x",
        "<pre><span title='",
        "<svg><foreignObject><script>",
        "<!-- x",
    ] {
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
    assert_eq!("<p> 中文 😀 </p><script>a();</script> <".as_bytes(), output.into_inner().unwrap());
}

#[test]
fn javascript_slashes_keep_regex_and_division_meaning() {
    test_cases(
        &[
            (
                "<script>debugger\n/a  b/.test(s)</script>",
                "<script> debugger\n/a  b/.test(s) </script>",
            ),
            (
                "<script>let n={valueOf(){return 2}}/ /a  b/.test(s);</script>",
                "<script>let n = { valueOf() { return 2 } } / /a  b/.test(s);</script>",
            ),
            ("<script>of/ /a  b/.test(s);</script>", "<script>of / /a  b/.test(s);</script>"),
            (
                "<script type=module>await/a  b/;</script>",
                "<script type=module>await /a  b/;</script>",
            ),
            (
                "<script type=module>export default/a  b/;</script>",
                "<script type=module>export default /a  b/;</script>",
            ),
            (
                "<script type=module>import a from\"x\"\n/a  b/.test(s)</script>",
                "<script type=module>import a from \"x\"\n/a  b/.test(s)</script>",
            ),
            (
                "<script>var x/*!\n*/ /a  b/.test(s)</script>",
                "<script>var x /*!\n*/ /a  b/.test(s)</script>",
            ),
            (
                "<script>var x\n/*! c */ /a  b/.test(s)</script>",
                "<script>var x\n/*! c */ /a  b/.test(s)</script>",
            ),
            (
                "<script>var x<!-- c\n/a  b/.test(s)</script>",
                "<script>var x <!-- c\n/a  b/.test(s)</script>",
            ),
        ],
        default_options,
    );
}

#[test]
fn javascript_unicode_whitespace_keeps_token_boundaries() {
    for whitespace in [
        '\u{00A0}', '\u{1680}', '\u{2000}', '\u{200A}', '\u{2028}', '\u{2029}', '\u{202F}',
        '\u{205F}', '\u{3000}', '\u{FEFF}',
    ] {
        test_cases(
            &[(
                "<script>const r=/a  b/;r.test(s)</script>",
                &format!("<script>const r ={whitespace}/a  b/;r.test(s)</script>"),
            )],
            default_options,
        );
    }
    test_cases(
        &[
            (
                "<script>function f(){return\n/a  b/}</script>",
                "<script>function f(){return\u{2028}/a  b/}</script>",
            ),
            ("<script>let 名字=é+α;</script>", "<script>let 名字 = é + α;</script>"),
            ("<script>let a\u{200C}b=あ+K;</script>", "<script>let a\u{200C}b = あ + K;</script>"),
        ],
        default_options,
    );
}

#[test]
fn script_double_escaped_end_tags_stay_in_content() {
    let input = "<script><!--\nlet s=\"<script></script>\";console.log(\"a  \
                 b\");\n//-->\n</script><p>x   y</p>";
    let expected = "<script><!--\nlet s=\"<script></script>\";console.log(\"a  \
                    b\");\n//-->\n</script><p>x y</p>";
    test_cases(&[(expected, input)], default_options);
    test_cases(&[(expected, input)], preserve_embedded);
    for input in [
        "<script><!--\nlet s='<ScRiPt ></sCrIpTx></sCrIpT >';\n//-->\n</script>",
        "<script><!--\nlet s='<script>-->';\na();\n</script>",
    ] {
        test_cases(&[(input, input)], default_options);
        test_cases(&[(input, input)], preserve_embedded);
    }
    test_cases(
        &[(
            "<pre><script><!--\nlet s='<script></script></pre>';\n//-->\n</script>  x</pre><p>y \
             z</p>",
            "<pre><script><!--\nlet s='<script></script></pre>';\n//-->\n</script>  x</pre><p>y   \
             z</p>",
        )],
        default_options,
    );
}

#[test]
fn other_raw_text_elements_keep_literal_markup() {
    for tag in ["xmp", "iframe", "noembed", "noframes"] {
        let input = format!("<{tag}>A<!--B--><b title='x  y'>C</b></{tag}><p>x   y</p>");
        let expected = format!("<{tag}>A<!--B--><b title='x  y'>C</b></{tag}><p>x y</p>");
        test_cases(&[(&expected, &input)], default_options);
    }
    test_cases(
        &[
            (
                "<title>A<!--B--><b title='x y'>C</b></title><p>x y</p>",
                "<title>A<!--B--><b title='x  y'>C</b></title><p>x   y</p>",
            ),
            ("<title>\nMy Page\n</title>", "<title>\n    My   Page\r\n</title>"),
        ],
        default_options,
    );
}

#[test]
fn unicode_tag_names_keep_attribute_values() {
    test_cases(
        &[("<x-中文 title=\"a  b\"></x-中文>", "<x-中文  title=\"a  b\" ></x-中文 >")],
        default_options,
    );
}

#[test]
fn literal_less_than_does_not_start_a_tag() {
    test_cases(
        &[
            ("a<3", "a<3"),
            ("a<>b", "a<>b"),
            ("a<=b", "a<=b"),
            ("<p></ <!-- x -->a</p>", "<p></ <!-- x -->a</p>"),
            ("<p><?x <!-- y -->a></p>", "<p><?x <!-- y -->a></p>"),
        ],
        default_options,
    );
    assert_eq!("a<3", minify("a<3").unwrap());
}

#[test]
fn bare_type_attributes_take_precedence() {
    test_cases(
        &[
            (
                "<script type type='text/plain'>a();</script>",
                "<script type type='text/plain'> a(); </script>",
            ),
            (
                "<script type type=module>let x=1;\n--> note\nx++;</script>",
                "<script type type=module>let x = 1;\n--> note\nx++;</script>",
            ),
        ],
        default_options,
    );
}

#[test]
fn foreign_content_keeps_start_tags_and_literal_markup() {
    test_cases(
        &[
            ("<SVG  class=' a  b ' /><p>x y</p>", "<SVG  class=' a  b ' /><p>x   y</p>"),
            (
                "<svg><style/><script/><title/></svg><p>x y</p>",
                "<svg><style/><script/><title/></svg><p>x   y</p>",
            ),
            (
                "<math><style/><script/></math><p>x y</p>",
                "<math><style/><script/></math><p>x   y</p>",
            ),
            (
                "<svg><svg><path /></svg>  x</svg><p>y z</p>",
                "<svg><svg><path /></svg>  x</svg><p>y   z</p>",
            ),
            (
                "<svg a='</svg>'><!-- </svg> --><![CDATA[a > b<!--c--></svg>]]><text>中文  \
                 😀</text></svg><p>x y</p>",
                "<svg a='</svg>'><!-- </svg> --><![CDATA[a > b<!--c--></svg>]]><text>中文  \
                 😀</text></svg><p>x   y</p>",
            ),
            (
                "<math><![CDATA[a > b<!--c--></math>]]></math><p>x y</p>",
                "<math><![CDATA[a > b<!--c--></math>]]></math><p>x   y</p>",
            ),
            ("<svg>\na\nb</svg><p>x y</p>", "<svg>\r\na\x00\rb</svg><p>x   y</p>"),
        ],
        default_options,
    );
}

#[test]
fn foreign_integration_points_follow_html_text_modes() {
    for tag in ["foreignObject", "desc", "title"] {
        let region = format!(
            "<svg><{tag}><div><br><script>let s = '</svg>';  run();</script><style>a::after {{ \
             content: '</svg>'; }}</style><textarea></svg>  \
             x</textarea><svg><style/></svg></div></{tag}></svg>"
        );
        test_cases(
            &[(&format!("{region}<p>x y</p>"), &format!("{region}<p>x   y</p>"))],
            default_options,
        );
    }

    for tag in ["mi", "mo", "mn", "ms", "mtext"] {
        let region = format!(
            "<math><{tag}><mglyph><style/></mglyph><malignmark/><script>let s = '</math>';  \
             run();</script><textarea></math>  x</textarea></{tag}></math>"
        );
        test_cases(
            &[(&format!("{region}<p>x y</p>"), &format!("{region}<p>x   y</p>"))],
            default_options,
        );
    }

    for encoding in [
        "text/html",
        "APPLICATION/XHTML+XML",
        "text&#47;html",
        "text&#47html",
        "text&#x00002Fhtml",
        "text&sol;html",
        "application/xhtml&plus;xml",
    ] {
        let region = format!(
            "<math><annotation-xml encoding='{encoding}' encoding='application/xml'><script>let s \
             = '</math>';  run();</script></annotation-xml></math>"
        );
        test_cases(
            &[(&format!("{region}<p>x y</p>"), &format!("{region}<p>x   y</p>"))],
            default_options,
        );
    }

    test_cases(
        &[
            (
                "<svg><foreignObject><![CDATA[</svg>]]><script>let s = \
                 '</svg>';</script></foreignObject></svg><p>x y</p>",
                "<svg><foreignObject><![CDATA[</svg>]]><script>let s = \
                 '</svg>';</script></foreignObject></svg><p>x   y</p>",
            ),
            (
                "<math><annotation-xml encoding='application/xml' \
                 encoding='text/html'><script/><style/></annotation-xml></math><p>x y</p>",
                "<math><annotation-xml encoding='application/xml' \
                 encoding='text/html'><script/><style/></annotation-xml></math><p>x   y</p>",
            ),
            (
                "<math><annotation-xml><svg><foreignObject><script>let s = \
                 '</math>';</script></foreignObject></svg></annotation-xml></math><p>x y</p>",
                "<math><annotation-xml><svg><foreignObject><script>let s = \
                 '</math>';</script></foreignObject></svg></annotation-xml></math><p>x   y</p>",
            ),
        ],
        default_options,
    );
}

#[test]
fn foreign_content_returns_to_preserved_html() {
    test_cases(
        &[(
            "<pre><svg><style/><foreignObject><script>let \
             s='</pre></svg>';</script></foreignObject></svg>  x</pre><p>y z</p>",
            "<pre><svg><style/><foreignObject><script>let \
             s='</pre></svg>';</script></foreignObject></svg>  x</pre><p>y   z</p>",
        )],
        default_options,
    );
    test_cases(
        &[(
            "<code><math><mtext><script>let s='</code></math>';</script></mtext></math>  \
             x</code><p>y z</p>",
            "<code><math><mtext><script>let s='</code></math>';</script></mtext></math>  \
             x</code><p>y   z</p>",
        )],
        preserve_code,
    );
}

#[test]
fn plaintext_keeps_everything_until_end_of_input() {
    for input in [
        "<plaintext>",
        "<plaintext>a  b<!--c-->",
        "<plaintext>中文  😀</plaintext><p>x   y</p>",
        "<pre><plaintext>a  b</pre><p>x   y</p>",
        "<svg><foreignObject><plaintext>a  b</svg><p>x   y</p>",
        "<pre><math><mtext><plaintext>a  b</pre></math>",
    ] {
        test_cases(&[(input, input)], default_options);
    }
    test_cases(&[("<plaintext>a\nb\nc", "<plaintext>a\rb\r\nc\x00")], default_options);
    let input = "<code><plaintext>a  b</code>";
    test_cases(&[(input, input)], preserve_code);
    assert_eq!("<plaintext>a  b<!--c-->", minify("<plaintext>a  b<!--c-->").unwrap());
}
