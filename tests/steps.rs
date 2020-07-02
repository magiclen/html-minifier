extern crate html_minifier;

use std::str::from_utf8_unchecked;

use html_minifier::HTMLMinifier;

fn test_enabled_all_options(cases: &[(&str, &str)]) {
    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();
        html_minifier.digest(test).unwrap();
        assert_eq!(expect, html_minifier.get_html(), "case {}", index);
    }

    let mut buffer = [0u8; 8];

    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();

        for c in test.chars() {
            html_minifier.digest(c.encode_utf8(&mut buffer)).unwrap();
        }

        assert_eq!(expect, html_minifier.get_html(), "case-chunk-1 {}", index);
    }

    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();

        let mut chars = test.chars();

        while let Some(c) = chars.next() {
            let mut length = c.encode_utf8(&mut buffer).len();

            if let Some(c) = chars.next() {
                length = length + c.encode_utf8(&mut buffer[length..]).len();
            }

            html_minifier.digest(unsafe { from_utf8_unchecked(&mut buffer[..length]) }).unwrap();
        }

        assert_eq!(expect, html_minifier.get_html(), "case-chunk-2 {}", index);
    }
}

fn test_disabled_all_options(cases: &[(&str, &str)]) {
    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();
        html_minifier.remove_comments = false;
        html_minifier.minify_code = false;

        html_minifier.digest(test).unwrap();
        assert_eq!(expect, html_minifier.get_html(), "case {}", index);
    }

    let mut buffer = [0u8; 8];

    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();
        html_minifier.remove_comments = false;
        html_minifier.minify_code = false;

        for c in test.chars() {
            html_minifier.digest(c.encode_utf8(&mut buffer)).unwrap();
        }

        assert_eq!(expect, html_minifier.get_html(), "case-chunk-1 {}", index);
    }

    for (index, (expect, test)) in cases.iter().copied().enumerate() {
        let mut html_minifier = HTMLMinifier::new();
        html_minifier.remove_comments = false;
        html_minifier.minify_code = false;

        let mut chars = test.chars();

        while let Some(c) = chars.next() {
            let mut length = c.encode_utf8(&mut buffer).len();

            if let Some(c) = chars.next() {
                length = length + c.encode_utf8(&mut buffer[length..]).len();
            }

            html_minifier.digest(unsafe { from_utf8_unchecked(&mut buffer[..length]) }).unwrap();
        }

        assert_eq!(expect, html_minifier.get_html(), "case-chunk-2 {}", index);
    }
}

#[test]
fn initial() {
    const CASES: [(&str, &str); 3] = [("", ""), ("", "   "), ("1", "  1")];

    test_enabled_all_options(&CASES);
}

#[test]
fn initial_remain_one_whitespace() {
    const CASES: [(&str, &str); 2] = [("1", "1 "), ("1", "1\t")];

    test_enabled_all_options(&CASES);
}

#[test]
fn initial_ignore_whitespace() {
    const CASES: [(&str, &str); 2] = [("1 23", "1  23"), ("1 234 567", "1  234   567")];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_initial() {
    const CASES: [(&str, &str); 6] =
        [("", "<>"), ("123", "123<>"), ("<a", "<a"), ("<", "< "), ("<", "<\t"), ("<!", "<!")];

    test_enabled_all_options(&CASES);
}

#[test]
fn end_tag_initial() {
    const CASES: [(&str, &str); 5] =
        [("", "</>"), ("123", "123</>"), ("</a", "</a"), ("</", "</ "), ("</", "</\t")];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag() {
    const CASES: [(&str, &str); 4] =
        [("<aaa", "<aaa"), ("<aaa", "<aaa "), ("<aaa/", "<aaa/"), ("<aaa>", "<aaa>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_in() {
    const CASES: [(&str, &str); 6] = [
        ("<aaa/", "<aaa /"),
        ("<aaa/", "<aaa   /"),
        ("<aaa>", "<aaa >"),
        ("<aaa>", "<aaa   >"),
        ("<aaa a", "<aaa a"),
        ("<aaa a", "<aaa   a"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_attribute_name() {
    const CASES: [(&str, &str); 5] = [
        ("<aaa abc", "<aaa abc"),
        ("<aaa abc/", "<aaa abc/"),
        ("<aaa abc>", "<aaa abc>"),
        ("<aaa abc=", "<aaa abc="),
        ("<aaa abc", "<aaa abc "),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_attribute_name_waiting_value() {
    const CASES: [(&str, &str); 6] = [
        ("<aaa abc/", "<aaa abc /"),
        ("<aaa abc/", "<aaa abc   /"),
        ("<aaa abc>", "<aaa abc >"),
        ("<aaa abc>", "<aaa abc   >"),
        ("<aaa abc=", "<aaa abc ="),
        ("<aaa abc=", "<aaa abc   ="),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_attribute_value_initial() {
    const CASES: [(&str, &str); 7] = [
        ("<aaa abc/", "<aaa abc=/"),
        ("<aaa abc>", "<aaa abc=>"),
        ("<aaa abc=\"", "<aaa abc=\""),
        ("<aaa abc='", "<aaa abc='"),
        ("<aaa abc='", "<aaa abc= '"),
        ("<aaa abc=", "<aaa abc= "),
        ("<aaa abc=v", "<aaa abc=v"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_quoted_attribute_value() {
    const CASES: [(&str, &str); 9] = [
        ("<aaa abc=\"v", "<aaa abc=\"v"),
        ("<aaa abc='v", "<aaa abc='v"),
        ("<aaa abc='123   456'", "<aaa abc='123   456'"),
        ("<aaa class='123 456'", "<aaa class='  123   456  '"),
        ("<aaa type='123   456'", "<aaa type='123   456'"),
        ("<style type='123 456'", "<style type='  123   456  '"),
        ("<script type='123 456'", "<script type='  123   456  '"),
        ("<aaa abc", "<aaa abc=''"),
        ("<aaa class", "<aaa class=''"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn start_tag_unquoted_attribute_value() {
    const CASES: [(&str, &str); 3] = [
        ("<aaa abc=vv", "<aaa abc=vv"),
        ("<aaa abc=vv>", "<aaa abc=vv>"),
        ("<aaa abc=vv", "<aaa abc=vv "),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn end_tag() {
    const CASES: [(&str, &str); 3] = [("</aaa", "</aaa"), ("</aaa", "</aaa"), ("</aaa>", "</aaa>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn tag_end() {
    const CASES: [(&str, &str); 4] = [
        ("<aaa/", "<aaa/123"),
        ("<aaa/>", "<aaa/>"),
        ("</aaa", "</aaa 123"),
        ("</aaa>", "</aaa 123>"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn doctype() {
    const CASES: [(&str, &str); 5] = [
        ("<!aaa", "<!aaa"),
        ("<!aaa   ", "<!aaa   "),
        ("<!aaa>", "<!aaa>"),
        ("<!aaa   >", "<!aaa   >"),
        ("", "<!--"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn comment() {
    const CASES: [(&str, &str); 2] = [("123", "1<!---->23"), ("", "<!--123-->")];

    test_enabled_all_options(&CASES);

    const CASES2: [(&str, &str); 2] = [("1<!---->23", "1<!---->23"), ("<!--123-->", "<!--123-->")];

    test_disabled_all_options(&CASES2);
}

#[test]
fn script_default() {
    const CASES: [(&str, &str); 2] = [
        (
            "<script type='application/ecmascript'>   alert('1234!')    ;   </script>",
            "<script type='application/ecmascript'>   alert('1234!')    ;   </script>",
        ),
        (
            "<script type='application/ecmascript'>   alert('1234!')    ;   </script>",
            "<script type='application/ecmascript'>   alert('1234!')    ;   </script  >",
        ),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn script_javascript() {
    const CASES: [(&str, &str); 3] = [
        (
            "<script type='application/javascript'>alert('1234!')</script>",
            "<script type='application/javascript'>   alert('1234!')    ;   </script>",
        ),
        ("<script>alert('1234!')</script>", "<script>   alert('1234!')    ;   </script  >"),
        ("<script>alert('1234!')</script>", "<script>alert('1234!');</script>"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn style_default() {
    const CASES: [(&str, &str); 2] = [
        (
            "<style type='text/x-scss'>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style>",
            "<style type='text/x-scss'>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style>",
        ),
        (
            "<style type='text/x-scss'>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style>",
            "<style type='text/x-scss'>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style  >",
        ),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn style_css() {
    const CASES: [(&str, &str); 3] = [
        (
            "<style type='text/css'>h1{color:blue;font-family:verdana;font-size:300%;}</style>",
            "<style type='text/css'>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style>",
        ),
        (
            "<style>h1{color:blue;font-family:verdana;font-size:300%;}</style>",
            "<style>
h1 {
    color: blue;
    font-family: verdana;
    font-size: 300%;
}
</style  >",
        ),
        (
            "<style>h1{color:blue;font-family:verdana;font-size:300%;}</style>",
            "<style>h1{color:blue;font-family:verdana;font-size:300%;}</style>",
        ),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn pre() {
    const CASES: [(&str, &str); 2] = [
        ("<pre>   alert('1234!')    ;   </pre>", "<pre>   alert('1234!')    ;   </pre>"),
        ("<pre>   alert('1234!')    ;   </pre>", "<pre>   alert('1234!')    ;   </pre  >"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn code() {
    const CASES: [(&str, &str); 2] = [
        ("<code> alert('1234!') ; </code>", "<code>   alert('1234!')    ;   </code>"),
        ("<code> alert('1234!') ; </code>", "<code>   alert('1234!')    ;   </code  >"),
    ];

    test_enabled_all_options(&CASES);

    const CASES2: [(&str, &str); 2] = [
        ("<code>   alert('1234!')    ;   </code>", "<code>   alert('1234!')    ;   </code>"),
        ("<code>   alert('1234!')    ;   </code>", "<code>   alert('1234!')    ;   </code  >"),
    ];

    test_disabled_all_options(&CASES2);
}

#[test]
fn textarea() {
    const CASES: [(&str, &str); 2] = [
        (
            "<textarea>   alert('1234!')    ;   </textarea>",
            "<textarea>   alert('1234!')    ;   </textarea>",
        ),
        (
            "<textarea>   alert('1234!')    ;   </textarea>",
            "<textarea>   alert('1234!')    ;   </textarea  >",
        ),
    ];

    test_enabled_all_options(&CASES);
}

// TODO -----Width 2-----

#[test]
fn width_2_initial() {
    const CASES: [(&str, &str); 1] = [("é", "  é")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_initial_remain_one_whitespace() {
    const CASES: [(&str, &str); 2] = [("é", "é "), ("é", "é\t")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_initial_ignore_whitespace() {
    const CASES: [(&str, &str); 2] = [("é éé", "é  éé"), ("é ééé ééé", "é  ééé   ééé")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_initial() {
    const CASES: [(&str, &str); 1] = [("<é >", "<é >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_end_tag_initial() {
    const CASES: [(&str, &str); 1] = [("</é >", "</é >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag() {
    const CASES: [(&str, &str); 1] = [("<aé >", "<aé >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_end_tag() {
    const CASES: [(&str, &str); 1] = [("</aé >", "</aé >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_in() {
    const CASES: [(&str, &str); 1] = [("<a é>", "<a é >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_attribute_name() {
    const CASES: [(&str, &str); 1] = [("<a ééé>", "<a ééé >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_attribute_name_waiting_value() {
    const CASES: [(&str, &str); 1] = [("<a a é>", "<a a é >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_attribute_value_initial() {
    const CASES: [(&str, &str); 1] = [("<a é=é>", "<a é=é>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_quoted_attribute_value() {
    const CASES: [(&str, &str); 1] = [("<a é='ééé é'>", "<a é='ééé é' >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_start_tag_unquoted_attribute_value() {
    const CASES: [(&str, &str); 1] = [("<a é=ééé>", "<a é=ééé >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_tag_end() {
    const CASES: [(&str, &str); 3] =
        [("<aaa/", "<aaa/ééé"), ("</aaa", "</aaa ééé"), ("</aaa>", "</aaa ééé>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_doctype() {
    const CASES: [(&str, &str); 1] = [("<!ééé", "<!ééé")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_comment() {
    const CASES: [(&str, &str); 2] = [("ééé", "é<!---->éé"), ("", "<!--ééé-->")];

    test_enabled_all_options(&CASES);

    const CASES2: [(&str, &str); 2] = [("é<!---->éé", "é<!---->éé"), ("<!--ééé-->", "<!--ééé-->")];

    test_disabled_all_options(&CASES2);
}

#[test]
fn width_2_script_default() {
    const CASES: [(&str, &str); 2] = [
        (
            "<script type='application/ecmascript'>   é é   </script>",
            "<script type='application/ecmascript'>   é é   </script>",
        ),
        (
            "<script type='application/ecmascript'>   é é   </script>",
            "<script type='application/ecmascript'>   é é   </script  >",
        ),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_2_script_javascript() {
    const CASES: [(&str, &str); 3] = [
        (
            "<script type='application/javascript'>alert('ééé!')</script>",
            "<script type='application/javascript'>   alert('ééé!')    ;   </script>",
        ),
        ("<script>alert('ééé!')</script>", "<script>   alert('ééé!')    ;   </script  >"),
        ("<script>alert('ééé!')</script>", "<script>alert('ééé!');</script>"),
    ];

    test_enabled_all_options(&CASES);
}

// TODO -----Width n (3 & 4)-----

#[test]
fn width_n_initial() {
    const CASES: [(&str, &str); 1] = [("中", "  中")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_initial_remain_one_whitespace() {
    const CASES: [(&str, &str); 2] = [("中", "中 "), ("中", "中\t")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_initial_ignore_whitespace() {
    const CASES: [(&str, &str); 8] = [
        ("中 中中", "中  中中"),
        ("中 中中中 中中中", "中  中中中   中中中"),
        ("中中", "中\n\t 中"),
        ("中中", "中 \n\t 中"),
        ("中 a", "中\n\t a"),
        ("中 a", "中 \n\t a"),
        ("a 中", "a\n\t 中"),
        ("a 中", "a \n\t 中"),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_initial() {
    const CASES: [(&str, &str); 1] = [("<中 >", "<中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_end_tag_initial() {
    const CASES: [(&str, &str); 1] = [("</中 >", "</中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag() {
    const CASES: [(&str, &str); 1] = [("<a中 >", "<a中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_end_tag() {
    const CASES: [(&str, &str); 1] = [("</a中 >", "</a中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_in() {
    const CASES: [(&str, &str); 1] = [("<a 中>", "<a 中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_attribute_name() {
    const CASES: [(&str, &str); 1] = [("<a 中中中>", "<a 中中中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_attribute_name_waiting_value() {
    const CASES: [(&str, &str); 1] = [("<a a 中>", "<a a 中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_attribute_value_initial() {
    const CASES: [(&str, &str); 1] = [("<a 中=中>", "<a 中=中>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_quoted_attribute_value() {
    const CASES: [(&str, &str); 1] = [("<a 中='中中中 中'>", "<a 中='中中中 中' >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_start_tag_unquoted_attribute_value() {
    const CASES: [(&str, &str); 1] = [("<a 中=中中中>", "<a 中=中中中 >")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_tag_end() {
    const CASES: [(&str, &str); 3] =
        [("<aaa/", "<aaa/中中中"), ("</aaa", "</aaa 中中中"), ("</aaa>", "</aaa 中中中>")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_doctype() {
    const CASES: [(&str, &str); 1] = [("<!中中中", "<!中中中")];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_comment() {
    const CASES: [(&str, &str); 2] = [("中中中", "中<!---->中中"), ("", "<!--中中中-->")];

    test_enabled_all_options(&CASES);

    const CASES2: [(&str, &str); 2] =
        [("中<!---->中中", "中<!---->中中"), ("<!--中中中-->", "<!--中中中-->")];

    test_disabled_all_options(&CASES2);
}

#[test]
fn width_n_script_default() {
    const CASES: [(&str, &str); 2] = [
        (
            "<script type='application/ecmascript'>   中 中   </script>",
            "<script type='application/ecmascript'>   中 中   </script>",
        ),
        (
            "<script type='application/ecmascript'>   中 中   </script>",
            "<script type='application/ecmascript'>   中 中   </script  >",
        ),
    ];

    test_enabled_all_options(&CASES);
}

#[test]
fn width_n_script_javascript() {
    const CASES: [(&str, &str); 3] = [
        (
            "<script type='application/javascript'>alert('中中中!')</script>",
            "<script type='application/javascript'>   alert('中中中!')    ;   </script>",
        ),
        ("<script>alert('中中中!')</script>", "<script>   alert('中中中!')    ;   </script  >"),
        ("<script>alert('中中中!')</script>", "<script>alert('中中中!');</script>"),
    ];

    test_enabled_all_options(&CASES);
}
