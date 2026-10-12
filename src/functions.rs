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
