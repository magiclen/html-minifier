use std::{
    fmt::{self, Formatter},
    str::from_utf8,
};

#[inline]
pub(crate) fn str_bytes_fmt(v: &[u8], f: &mut Formatter) -> Result<(), fmt::Error> {
    match from_utf8(v) {
        Ok(text) => fmt::Debug::fmt(text, f),
        Err(_) => fmt::Debug::fmt(v, f),
    }
}

#[inline]
pub(crate) const fn is_whitespace(e: u8) -> bool {
    matches!(e, b'\t' | b'\n' | 0x0C | b'\r' | b' ')
}

#[inline]
pub(crate) const fn is_ascii_control(e: u8) -> bool {
    matches!(e, 0..=8 | 11 | 14..=31 | 127)
}

#[inline]
pub(crate) fn is_end_tag(e: u8, end_tag: &[u8], counter: &mut u8) -> bool {
    let p = usize::from(*counter);

    if p == end_tag.len() && (e == b'>' || e == b'/' || is_whitespace(e)) {
        return true;
    }

    if p < end_tag.len() && e.eq_ignore_ascii_case(&end_tag[p]) {
        *counter += 1;
    } else {
        *counter = u8::from(e == b'<');
    }

    false
}

#[inline]
pub(crate) fn ends_with_ignore_ascii_case(bytes: &[u8], suffix: &[u8]) -> bool {
    bytes.len() >= suffix.len() && bytes[bytes.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// The state of a comment right after `<!--`.
pub(crate) const COMMENT_START: u8 = 4;

/// Follow the comment states of the HTML tokenizer that can end a comment.
/// The state is 0 in the text, 1 after `-`, 2 after `--`, 3 after `--!`, 4 after `<!--`, and 5 after `<!---`.
/// Return `None` when the comment ends.
#[inline]
pub(crate) const fn next_comment_state(state: u8, e: u8) -> Option<u8> {
    match (state, e) {
        (2..=5, b'>') => None,
        (4, b'-') => Some(5),
        (1 | 2 | 5, b'-') => Some(2),
        (0 | 3, b'-') => Some(1),
        (2, b'!') => Some(3),
        _ => Some(0),
    }
}

/// Get the end tag that ends the raw text of an element, such as `</script` for `script`.
pub(crate) fn raw_text_end_tag(tag: &[u8]) -> Option<&'static [u8]> {
    let end_tag: &'static [u8] = match tag {
        b"script" => b"</script",
        b"style" => b"</style",
        b"textarea" => b"</textarea",
        b"title" => b"</title",
        b"xmp" => b"</xmp",
        b"iframe" => b"</iframe",
        b"noembed" => b"</noembed",
        b"noframes" => b"</noframes",
        _ => return None,
    };

    Some(end_tag)
}
