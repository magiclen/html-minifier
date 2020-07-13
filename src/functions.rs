use core::fmt::{self, Formatter};
use core::str::{from_utf8_unchecked, FromStr};

#[inline]
pub(crate) fn str_bytes_fmt(v: &[u8], f: &mut Formatter) -> Result<(), fmt::Error> {
    f.write_fmt(format_args!("{:?}", unsafe { from_utf8_unchecked(v) }))
}

#[inline]
pub(crate) fn is_whitespace(e: u8) -> bool {
    match e {
        0x09..=0x0D | 0x1C..=0x20 => true,
        _ => false,
    }
}

#[inline]
pub(crate) fn is_ascii_control(e: u8) -> bool {
    match e {
        0..=8 | 11..=31 | 127 => true,
        _ => false,
    }
}

#[inline]
pub(crate) fn is_cj(c: char) -> bool {
    (c >= '\u{2E80}' && c <= '\u{2EF3}')  // CJK Radicals Supplement
        || (c >= '\u{2F00}' && c <= '\u{2FDF}') // Kangxi Radicals
        || (c >= '\u{2FF0}' && c <= '\u{2FD5}') // Ideographic Description Characters
        || (c >= '\u{3005}' && c <= '\u{303B}') // CJK Symbols and Punctuation
        || (c >= '\u{3040}' && c <= '\u{309F}') // Hiragana
        || (c >= '\u{30A0}' && c <= '\u{30FF}') // Katakana
        || (c >= '\u{3100}' && c <= '\u{312F}') // Bopomofo
        || (c >= '\u{31A0}' && c <= '\u{31BA}') // Bopomofo Extended
        || (c >= '\u{31C0}' && c <= '\u{31E3}') // CJK Strokes
        || (c >= '\u{31F0}' && c <= '\u{31FF}') // Katakana Phonetic Extensions
        || (c >= '\u{32D0}' && c <= '\u{32FF}') // Circled Katakana
        || (c >= '\u{3300}' && c <= '\u{33FF}') // CJK Compatibility
        || (c >= '\u{3400}' && c <= '\u{4DBF}') // CJK Unified Ideographs Extension A
        || (c >= '\u{4E00}' && c <= '\u{9FFF}') // CJK Unified Ideographs
        || (c >= '\u{A000}' && c <= '\u{A48C}') // Yi Syllables
        || (c >= '\u{A490}' && c <= '\u{A4C6}') // Yi Radicals
        || (c >= '\u{F900}' && c <= '\u{FAFF}') // CJK Compatibility Ideographs
        || (c >= '\u{FE30}' && c <= '\u{FE4F}') // CJK Compatibility Forms
        || (c >= '\u{FF65}' && c <= '\u{FF9D}') // Halfwidth And Fullwidth Forms
        || (c >= '\u{1B000}' && c <= '\u{1B0FE}') // Kana Supplement
        || (c >= '\u{1B100}' && c <= '\u{1B11E}') // Kana Extended A
        || (c >= '\u{1B150}' && c <= '\u{1B152}') // Small Kana Extension
        || (c >= '\u{1B164}' && c <= '\u{1B167}') // Small Kana Extension
        || c == '\u{1F200}' // Enclosed Ideographic Supplement
        || (c >= '\u{20000}' && c <= '\u{2A6DF}') // CJK Unified Ideographs Extension B
        || (c >= '\u{2A700}' && c <= '\u{2B73F}') // CJK Unified Ideographs Extension C
        || (c >= '\u{2B740}' && c <= '\u{2B81F}') // CJK Unified Ideographs Extension D
        || (c >= '\u{2B820}' && c <= '\u{2CEAF}') // CJK Unified Ideographs Extension E
        || (c >= '\u{2CEB0}' && c <= '\u{2EBEF}') // CJK Unified Ideographs Extension F
        || (c >= '\u{2F800}' && c <= '\u{2FA1F}') // CJK Compatibility Ideographs Supplement
}

#[inline]
pub(crate) fn is_bytes_cj(bytes: &[u8]) -> bool {
    match char::from_str(unsafe { from_utf8_unchecked(bytes) }) {
        Ok(c) => is_cj(c),
        Err(_) => false,
    }
}
