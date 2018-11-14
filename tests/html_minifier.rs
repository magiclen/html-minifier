extern crate html_minifier;

use html_minifier::HTMLMinifier;

#[test]
fn remove_ascii_control() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("\x00<html>").unwrap();

    assert_eq!("<html>", html_minifier.get_html());
}

#[test]
fn remove_starting_spaces() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"
            <html>"#).unwrap();

    assert_eq!("<html>", html_minifier.get_html());
}

#[test]
fn remove_ending_spaces() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"</html>      "#).unwrap();

    assert_eq!("</html>", html_minifier.get_html());
}

#[test]
fn remove_starting_spaces_in_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<     html>"#).unwrap();

    assert_eq!("<html>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"</    html>"#).unwrap();

    assert_eq!("</html>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<    /html>"#).unwrap();

    assert_eq!("</html>", html_minifier.get_html());
}

#[test]
fn remove_endding_spaces_in_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<html    >"#).unwrap();

    assert_eq!("<html>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<html   data="5   123"  class=""     >"#).unwrap();

    assert_eq!(r#"<html data="5   123" class="">"#, html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"</html    >"#).unwrap();

    assert_eq!("</html>", html_minifier.get_html());
}

#[test]
fn self_closed_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<html/>"#).unwrap();

    assert_eq!("<html/>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<html   />"#).unwrap();

    assert_eq!("<html/>", html_minifier.get_html());
}

#[test]
fn remove_comments() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<!-- 123456 --><html>"#).unwrap();

    assert_eq!("<html>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<!-- 123456 --><html><!-- 123456 --></html>"#).unwrap();

    assert_eq!("<html></html>", html_minifier.get_html());
}

#[test]
fn replace_spaces_to_one_before_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"1234567     <span>"#).unwrap();

    assert_eq!("1234567 <span>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"1234567</span>     <span>"#).unwrap();

    assert_eq!("1234567</span> <span>", html_minifier.get_html());
}

#[test]
fn remove_lines_and_spaces_before_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("1234567\n\n<span>").unwrap();

    assert_eq!("1234567<span>", html_minifier.get_html());

    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("</div>\n     <span>").unwrap();

    assert_eq!("</div><span>", html_minifier.get_html());
}

#[test]
fn replace_spaces_to_one_after_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("<span>   1234567").unwrap();

    assert_eq!("<span> 1234567", html_minifier.get_html());
}

#[test]
fn remove_lines_and_spaces_after_a_tag() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("<span>\n   1234567").unwrap();

    assert_eq!("<span>1234567", html_minifier.get_html());
}

#[test]
fn inline() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("<p>\n\n123456\n\n<label>\n789\n<label>\n\n</p>").unwrap();

    assert_eq!("<p>123456<label>789<label></p>", html_minifier.get_html());
}

#[test]
fn text_mix_basic() {
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
}

#[test]
fn pre() {
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
}

#[test]
fn pre_ascii_control_characters() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("<pre>\t\t1234567\n\t\t\t\x00890</pre>").unwrap();

    assert_eq!("<pre>\t\t1234567\n\t\t\t890</pre>", html_minifier.get_html());
}

#[test]
fn script() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest(r#"<script>
        alert('1234!')    ;

        </script>"#).unwrap();

    assert_eq!("<script>alert('1234!');</script>", html_minifier.get_html());
}

#[test]
fn style() {
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
}