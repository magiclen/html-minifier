extern crate html_minifier;

use html_minifier::HTMLMinifier;

#[test]
fn concat() {
    let mut minifier = HTMLMinifier::new();

    minifier.digest("1").unwrap();
    minifier.digest("23").unwrap();
    minifier.digest("4").unwrap();

    assert_eq!("1234", minifier.get_html());
}

#[test]
fn reset() {
    let mut minifier = HTMLMinifier::new();

    minifier.digest("1").unwrap();

    minifier.reset();

    minifier.digest("23").unwrap();

    assert_eq!("23", minifier.get_html());
}

#[test]
fn remove_ascii_control_characters() {
    let mut minifier = HTMLMinifier::new();

    minifier.digest("\x00<html>").unwrap();

    assert_eq!("<html>", minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_start() {
    let mut minifier = HTMLMinifier::new();

    minifier.digest("  \n \t123").unwrap();

    assert_eq!("123", minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_end() {
    let mut minifier = HTMLMinifier::new();

    minifier.digest("123  \n \t").unwrap();

    assert_eq!("123", minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_content() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("a   b").unwrap();

        assert_eq!("a b", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("a \n\t\n\n\t\t  b").unwrap();

        assert_eq!("a b", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("a \n b").unwrap();

        assert_eq!("a b", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("a\n<span>b</span>").unwrap();

        assert_eq!("a <span>b</span>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("<span>a  </span>  <span>  b</span>").unwrap();

        assert_eq!("<span>a </span> <span> b</span>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("中   文").unwrap();

        assert_eq!("中 文", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("中 \n\t\n\n\t\t  文").unwrap();

        assert_eq!("中文", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("中 \n 文").unwrap();

        assert_eq!("中文", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("中\n<span>文</span>").unwrap();

        assert_eq!("中 <span>文</span>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("<span>中  </span>  <span>  文</span>").unwrap();

        assert_eq!("<span>中 </span> <span> 文</span>", minifier.get_html());
    }
}

#[test]
fn remove_useless_whitespaces_from_tag() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("<div  >").unwrap();

        assert_eq!("<div>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("<  div>").unwrap();

        assert_eq!("<div>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("< /  div   >").unwrap();

        assert_eq!("</div>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest("<   hr  /  >").unwrap();

        assert_eq!("<hr/>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest(r#"<div   id="name  xxx"    class="  col-1   col-md-5 ">"#).unwrap();

        assert_eq!(r#"<div id="name  xxx" class="col-1 col-md-5">"#, minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest(r#"<div   id="name  xxx"    class="">"#).unwrap();

        assert_eq!(r#"<div id="name  xxx" class="">"#, minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest(r#"<div   id="name  xxx"    class="  ">"#).unwrap();

        assert_eq!(r#"<div id="name  xxx" class="">"#, minifier.get_html());
    }

    minifier.reset();

    {
        minifier.digest(r#"< input type="text"  value="123   45"  / >"#).unwrap();

        assert_eq!(r#"<input type="text" value="123   45"/>"#, minifier.get_html());
    }
}

#[test]
fn remove_comments() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("<!-- 123456 --><html>").unwrap();

        assert_eq!("<html>", minifier.get_html());
    }

    minifier.reset();
    minifier.remove_comments = false;

    {
        minifier.digest("<!-- 1234   56 --><html>").unwrap();

        assert_eq!("<!-- 1234   56 --><html>", minifier.get_html());
    }
}

#[test]
fn text_mix_basic() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"
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
        "#,
            )
            .unwrap();

        assert_eq!(
            r#"<!DOCTYPE html> <html lang=en> <head> <head name=viewport> </head> <body class="container bg-light"> <input type="text" value='123   456'/> 123456 <b>big</b> 789 </body> </html>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn pre() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"<pre   lang="html"  >
    <html>
        1234567
    </html></ pre>
    <div>
        1234567
    </div>
    <pre>
        1234567
    < /pre   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<pre lang="html">
    <html>
        1234567
    </html></pre> <div> 1234567 </div> <pre>
        1234567
    </pre>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn pre_ascii_control_characters() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("<pre>\t\t1234567\n\t\t\t\x00890</pre>").unwrap();

        assert_eq!("<pre>\t\t1234567\n\t\t\t890</pre>", minifier.get_html());
    }
}

#[test]
fn code() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"<code   lang="html"  >
    <html>
        1234567
    </html></ code>
    <div>
        1234567
    </div>
    <code>
        1234567
    < /code   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<code lang="html">
    <html>
        1234567
    </html></code> <div> 1234567 </div> <code>
        1234567
    </code>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn code_ascii_control_characters() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("<code>\t\t1234567\n\t\t\t\x00890</code>").unwrap();

        assert_eq!("<code>\t\t1234567\n\t\t\t890</code>", minifier.get_html());
    }
}

#[test]
fn textarea() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"<textarea   class="control"  >Hi,

This is a textarea.
You can write multi-line messages here.
</ textarea>
    <div>
        1234567
    </div>
    <textarea>
        1234567
    < /textarea   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<textarea class="control">Hi,

This is a textarea.
You can write multi-line messages here.
</textarea> <div> 1234567 </div> <textarea>
        1234567
    </textarea>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn textarea_ascii_control_characters() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier.digest("<textarea>\t\t1234567\n\t\t\t\x00890</textarea>").unwrap();

        assert_eq!("<textarea>\t\t1234567\n\t\t\t890</textarea>", minifier.get_html());
    }
}

#[test]
fn javascript() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"< script  >
        alert('1234!')    ;

        < /  script >"#,
            )
            .unwrap();

        assert_eq!("<script>alert('1234!')</script>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier
            .digest(
                r#"< script  type="  application/javascript  "  >
        alert('1234!')    ;

        < /  script >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<script type="application/javascript">alert('1234!')</script>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn unsupported_script_type() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"< script  type="  application/ecmascript  "  >
        alert('1234!')    ;

        < /  script >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<script type="application/ecmascript">
        alert('1234!')    ;

        </script>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn css() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"< style>
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
        < /  style >"#,
            )
            .unwrap();

        assert_eq!("<style>h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>", minifier.get_html());
    }

    minifier.reset();

    {
        minifier
            .digest(
                r#"< style  type="  text/css  "  >
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
        < /  style >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<style type="text/css">h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>"#,
            minifier.get_html()
        );
    }
}

#[test]
fn unsupported_style_type() {
    let mut minifier = HTMLMinifier::new();

    {
        minifier
            .digest(
                r#"< style  type="  text/x-scss  "  >
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
        < /  style >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<style type="text/x-scss">
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
        </style>"#,
            minifier.get_html()
        );
    }
}