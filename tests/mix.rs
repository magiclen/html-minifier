use html_minifier::HTMLMinifier;

#[test]
fn reset() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("1").unwrap();

    html_minifier.reset();

    html_minifier.digest("23").unwrap();

    assert_eq!(b"23", html_minifier.get_html());
}

#[test]
fn remove_ascii_control_characters() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("\x00<html>").unwrap();

    assert_eq!(b"<html>", html_minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_start() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("  \n \t123").unwrap();

    assert_eq!(b"123", html_minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_end() {
    let mut html_minifier = HTMLMinifier::new();

    html_minifier.digest("123  \n \t").unwrap();

    assert_eq!(b"123", html_minifier.get_html());
}

#[test]
fn remove_useless_whitespaces_from_tag() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier.digest("<div  >").unwrap();

        assert_eq!(b"<div>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("</div   >").unwrap();

        assert_eq!(b"</div>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("<hr  /  >").unwrap();

        assert_eq!(b"<hr/>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest(r#"<div   id="name  xxx"    class="  col-1   col-md-5 ">"#).unwrap();

        assert_eq!(
            r#"<div id="name  xxx" class="col-1 col-md-5">"#.as_bytes(),
            html_minifier.get_html()
        );
    }

    html_minifier.reset();

    {
        html_minifier.digest(r#"<div   id="name  xxx"    class="">"#).unwrap();

        assert_eq!(r#"<div id="name  xxx" class>"#.as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest(r#"<div   id="name  xxx"    class="  ">"#).unwrap();

        assert_eq!(r#"<div id="name  xxx" class>"#.as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest(r#"<input type="text"  value="123   45"  / >"#).unwrap();

        assert_eq!(r#"<input type="text" value="123   45"/>"#.as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest(r#"<input type="text"  value=123  / >"#).unwrap();

        assert_eq!(br#"<input type="text" value=123 />"#.as_ref(), html_minifier.get_html());
    }
}

#[test]
fn remove_useless_whitespaces_from_content() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier.digest("a   b").unwrap();

        assert_eq!(b"a b", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("a \t\t\t  b").unwrap();

        assert_eq!(b"a b", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("a \n b").unwrap();

        assert_eq!(b"a\nb", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("a\n<span>b</span>").unwrap();

        assert_eq!(b"a\n<span>b</span>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("<span>a  </span>  <span>  b</span>").unwrap();

        assert_eq!(b"<span>a </span> <span> b</span>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("<a>1</a>\n /\n <a>2</a>").unwrap();

        assert_eq!(b"<a>1</a>\n/\n<a>2</a>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("中   文").unwrap();

        assert_eq!("中 文".as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("中 \t\t\t  文").unwrap();

        assert_eq!("中 文".as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("中 \n 文").unwrap();

        assert_eq!("中\n文".as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("中\n<span>文</span>").unwrap();

        assert_eq!("中\n<span>文</span>".as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier.digest("<span>中  </span>  <span>  文</span>").unwrap();

        assert_eq!("<span>中 </span> <span> 文</span>".as_bytes(), html_minifier.get_html());
    }
}

#[test]
fn text_mix_basic() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"
                <!DOCTYPE html>
                <html lang=en>
                    <head >
                        <meta  name=viewport  >
                    </head  >
                    <body     class="container    bg-light" >
                        <input type="text" value='123   456'    />
                        <!-- Content -->
                        123456 <b>big</b> 789

                    </body >
                </html >
        "#,
            )
            .unwrap();

        assert_eq!(
            r#"<!DOCTYPE html>
<html lang=en>
<head>
<meta name=viewport>
</head>
<body class="container bg-light">
<input type="text" value='123   456'/>
123456 <b>big</b> 789
</body>
</html>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn minify_css() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<style  >
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
        </style >"#,
            )
            .unwrap();

        assert_eq!("<style>h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>".as_bytes(), html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier
            .digest(
                r#"<style  type="  text/css  "  >
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
        </style >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<style type="text/css">h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>"#.as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn minify_javascript() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<script  >
        alert('1234!')    ;

        </script >"#,
            )
            .unwrap();

        assert_eq!(b"<script>alert('1234!')</script>", html_minifier.get_html());
    }

    html_minifier.reset();

    {
        html_minifier
            .digest(
                r#"<script  type="  application/javascript  "  >
        alert('1234!')    ;

        </script >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<script type="application/javascript">alert('1234!')</script>"#.as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn minify_javascript_css() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<script  >
        alert('1234!')    ;

        </script ><style  >
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
        </style >"#,
            )
            .unwrap();

        assert_eq!("<script>alert('1234!')</script><style>h1{color:blue;font-family:verdana;font-size:300%;}p{color:red;font-family:courier;font-size:160%;}</style>".as_bytes(), html_minifier.get_html());
    }
}

#[test]
fn preserve_pre() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<pre   lang="html"  >
    <html>
        1234567
    </html></pre>
    <div>
        1234567
    </div>
    <pre>
        1234567
    </pre   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<pre lang="html">
    <html>
        1234567
    </html></pre>
<div>
1234567
</div>
<pre>
        1234567
    </pre>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn preserve_code() {
    let mut html_minifier = HTMLMinifier::new();
    html_minifier.set_minify_code(false);

    {
        html_minifier
            .digest(
                r#"<code   lang="html"  >
    <html>
        1234567
    </html></code>
    <div>
        1234567
    </div>
    <code>
        1234567
    </code   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<code lang="html">
    <html>
        1234567
    </html></code>
<div>
1234567
</div>
<code>
        1234567
    </code>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn preserve_textarea() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<textarea   class="control"  >Hi,

This is a textarea.
You can write multi-line messages here.
</textarea>
    <div>
        1234567
    </div>
    <textarea>
        1234567
    </textarea   >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<textarea class="control">Hi,

This is a textarea.
You can write multi-line messages here.
</textarea>
<div>
1234567
</div>
<textarea>
        1234567
    </textarea>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn preserve_unsupported_script_type() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<script  type="  application/ecmascript  "  >
        alert('1234!')    ;

        </script >"#,
            )
            .unwrap();

        assert_eq!(
            r#"<script type="application/ecmascript">
        alert('1234!')    ;

        </script>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}

#[test]
fn preserve_unsupported_style_type() {
    let mut html_minifier = HTMLMinifier::new();

    {
        html_minifier
            .digest(
                r#"<style  type="  text/x-scss  "  >
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
        </style >"#,
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
        </style>"#
                .as_bytes(),
            html_minifier.get_html()
        );
    }
}
