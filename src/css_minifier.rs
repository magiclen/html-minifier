use std::mem::take;

use educe::Educe;

use crate::{HTMLMinifierError, HTMLWriter, comment::HeldComment, functions::*};

#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum CSSStep {
    #[educe(Default)]
    Normal,
    Slash,
    Comment,
    KeptComment,
    String,
    Escape,
    HexEscape,
    URLStart,
    URLBody,
    CustomStart,
    Custom,
}

/// Check whether a byte can be a part of a CSS name.
#[inline]
const fn is_name_byte(e: u8) -> bool {
    e.is_ascii_alphanumeric() || matches!(e, b'-' | b'_') || e >= 0x80
}

#[inline]
fn hex_digit_value(e: u8) -> u32 {
    char::from(e).to_digit(16).unwrap_or(0)
}

/// Minify the CSS content of a `style` element, which arrives in chunks.
/// Only comments and whitespace are removed, so the CSS keeps its meaning.
#[derive(Educe, Clone)]
#[educe(Debug, Default)]
pub(crate) struct CSSMinifier {
    step:           CSSStep,
    /// The last two written bytes, with the latest one at the end. 0 means nothing.
    last:           [u8; 2],
    /// Whether the latest written byte ends an escape, which then works like a name byte.
    last_escape:    bool,
    /// Whether the latest written byte ends a hex escape that can still take a whitespace byte.
    open_hex:       bool,
    /// Whether whitespace or comments are pending between two tokens.
    gap:            bool,
    /// Whether the pending gap includes whitespace.
    gap_whitespace: bool,
    /// Whether a name is being written.
    in_name:        bool,
    /// Whether the current name follows `#` or `@`, which makes a hash or an at-keyword.
    name_prefixed:  bool,
    /// The first character of the current name, in lowercase.
    name_first:     u8,
    /// The number of characters of the current name, up to 255.
    name_length:    u8,
    /// How many characters of the current name match `url`.
    url_matched:    u8,
    /// Whether the latest name starts with `--`, which is a custom property.
    custom_name:    bool,
    /// How many bytes of `<!--` have been written as tokens.
    cdo:            u8,
    /// The quote of the current string.
    quote:          u8,
    /// The state of an escape in a string, a URL or a custom property value.
    /// 1 is after a backslash, and 2 to 7 are in a hex escape with 1 to 6 hex digits.
    escape:         u8,
    hex_digits:     u8,
    hex_value:      u32,
    /// Whether the previous byte of a comment is `*`.
    comment_star:   bool,
    /// Whether whitespace has been removed at the start of a custom property value.
    custom_space:   bool,
    custom_depth:   u32,
    custom_quote:   u8,
    /// 1 after `/`, 2 in a comment, and 3 in a comment after `*`.
    custom_comment: u8,
    comment:        HeldComment,
}

impl CSSMinifier {
    /// Prepare for a new style sheet.
    #[inline]
    pub(crate) fn reset(&mut self) {
        let comment = take(&mut self.comment);

        *self = Self {
            comment,
            ..Self::default()
        };
    }

    #[inline]
    fn wrote(&mut self, e: u8) {
        self.last = [self.last[1], e];
        self.last_escape = false;
        self.open_hex = false;
    }

    /// Check whether the latest written token and a token starting with `next` become different tokens without anything between them.
    fn merges(&self, next: u8) -> bool {
        let [before, last] = self.last;

        ((self.last_escape || is_name_byte(last))
            && (is_name_byte(next) || matches!(next, b'(' | b'\\' | b'%' | b'+')))
            || (matches!(last, b'.' | b'+') && next.is_ascii_digit())
            || (matches!(last, b'+' | b'-') && next == b'.')
            || (last.is_ascii_digit() && next == b'.')
            || (matches!(last, b'#' | b'@') && (is_name_byte(next) || next == b'\\'))
            || (matches!(last, b'~' | b'|' | b'^' | b'$' | b'*') && next == b'=')
            || (last == b'|' && next == b'|')
            || (last == b'/' && next == b'*')
            || (last == b'<' && matches!(next, b'!' | b'/'))
            || (before == b'<' && last == b'!' && next == b'-')
            || (before == b'-' && last == b'-' && next == b'>')
    }

    /// Write what replaces the pending whitespace and comments before a token starting with `next`.
    fn write_gap(&mut self, next: u8, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        let last = self.last[1];

        // Nothing is needed at the start of the style sheet.
        if last != 0 {
            if self.gap_whitespace {
                // An escaped byte, such as `\:`, is a part of a name.
                let removable = (!self.last_escape
                    && matches!(
                        last,
                        b'{' | b'}' | b';' | b',' | b'(' | b'[' | b':' | b'>' | b'~'
                    ))
                    || matches!(next, b'{' | b'}' | b';' | b',' | b')' | b']' | b'>' | b'~');

                // The space stays if the tokens around it would merge, such as `-- >` and `~ =`.
                if !removable || self.merges(next) {
                    // A hex escape takes the first whitespace byte after it, so one more is needed.
                    if self.open_hex {
                        out.push(b' ')?;
                    }

                    out.push(b' ')?;
                    self.wrote(b' ');
                }
            } else if self.merges(next) {
                // A comment separates tokens without being whitespace, so an empty one is kept.
                out.push_bytes(b"/**/")?;
                self.last = *b"*/";
                self.last_escape = false;
                self.open_hex = false;
            }
        }

        self.gap = false;
        self.gap_whitespace = false;

        Ok(())
    }

    /// Track a byte after a backslash in a string, a URL or a custom property value.
    /// Return `false` if the escape has ended before the byte, so the byte has to be handled as usual.
    #[inline]
    fn continue_escape(&mut self, e: u8) -> bool {
        match self.escape {
            1 => {
                self.escape = if e.is_ascii_hexdigit() { 2 } else { 0 };

                true
            },
            2..=6 if e.is_ascii_hexdigit() => {
                self.escape += 1;

                true
            },
            _ => {
                // A hex escape takes the first whitespace byte after it.
                self.escape = 0;

                is_whitespace(e)
            },
        }
    }

    /// Track a character of the current name, which can come from an escape.
    #[inline]
    fn name_char(&mut self, c: u32) {
        let index = self.name_length;
        let lower = u8::try_from(c).map_or(0, |c| c.to_ascii_lowercase());

        self.url_matched = if !self.name_prefixed
            && index < 3
            && self.url_matched == index
            && lower == b"url"[usize::from(index)]
        {
            index + 1
        } else {
            0
        };

        match index {
            0 => self.name_first = lower,
            1 => self.custom_name = !self.name_prefixed && self.name_first == b'-' && lower == b'-',
            _ => (),
        }

        self.name_length = self.name_length.saturating_add(1);
    }

    /// Track a token byte that is written in the normal step.
    #[inline]
    fn write_token_byte(&mut self, e: u8) {
        let before = self.last[1];

        self.wrote(e);

        self.cdo = match (self.cdo, e) {
            (_, b'<') => 1,
            (1, b'!') => 2,
            (2, b'-') => 3,
            (3, b'-') => 4,
            _ => 0,
        };

        if is_name_byte(e) || e == b'\\' {
            if !self.in_name {
                // A name right after `#` or `@` is a hash or an at-keyword, which is never `url(` or a custom property.
                self.in_name = true;
                self.name_prefixed = matches!(before, b'#' | b'@');
                self.name_length = 0;
                self.url_matched = 0;
                self.custom_name = false;
            }

            if e == b'\\' {
                self.step = CSSStep::Escape;
            } else {
                self.name_char(u32::from(e));
            }

            if self.cdo == 4 {
                // `<!--` is a token of its own, so a name can start right after it.
                self.in_name = false;
                self.url_matched = 0;
                self.custom_name = false;
                self.cdo = 0;
            }
        } else {
            let url = self.in_name && self.url_matched == 3 && self.name_length == 3;

            self.in_name = false;
            self.url_matched = 0;

            match e {
                b'"' | b'\'' => {
                    self.quote = e;
                    self.escape = 0;
                    self.step = CSSStep::String;
                },
                b'(' if url => self.step = CSSStep::URLStart,
                b':' if self.custom_name => {
                    self.custom_space = false;
                    self.step = CSSStep::CustomStart;
                },
                _ => (),
            }

            self.custom_name = false;
        }
    }

    /// Input a chunk of CSS.
    pub(crate) fn feed(
        &mut self,
        bytes: &[u8],
        out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        // The start of the bytes that are written as they are.
        let mut run = 0;
        let mut p = 0;

        while p < bytes.len() {
            let e = bytes[p];

            match self.step {
                CSSStep::Normal => {
                    if is_whitespace(e) || e == b'/' {
                        if run < p {
                            out.push_bytes(&bytes[run..p])?;
                        }

                        run = p + 1;
                        self.in_name = false;
                        self.url_matched = 0;
                        self.cdo = 0;

                        if e == b'/' {
                            self.step = CSSStep::Slash;
                        } else {
                            self.gap = true;
                            self.gap_whitespace = true;
                        }
                    } else {
                        if self.gap {
                            self.write_gap(e, out)?;
                        }

                        self.write_token_byte(e);
                    }
                },
                CSSStep::Slash => {
                    if e == b'*' {
                        run = p + 1;
                        self.gap = true;
                        self.comment.start();
                        self.comment_star = false;
                        self.step = CSSStep::Comment;
                    } else {
                        // The pending `/` is a delimiter.
                        if self.gap {
                            self.write_gap(b'/', out)?;
                        }

                        out.push(b'/')?;
                        self.write_token_byte(b'/');
                        self.step = CSSStep::Normal;

                        continue;
                    }
                },
                CSSStep::Comment => {
                    run = p + 1;

                    if self.comment_star && e == b'/' {
                        // The comment is removed, and it stays a part of the gap.
                        self.step = CSSStep::Normal;
                    } else if self.comment.push(e) {
                        self.write_gap(b'/', out)?;

                        let held = self.comment.as_bytes();
                        out.push_bytes(held)?;
                        self.last = [held[held.len() - 2], held[held.len() - 1]];
                        self.last_escape = false;
                        self.open_hex = false;
                        self.step = CSSStep::KeptComment;
                    }

                    self.comment_star = e == b'*';
                },
                CSSStep::KeptComment => {
                    self.wrote(e);

                    if self.comment_star && e == b'/' {
                        self.step = CSSStep::Normal;
                    }

                    self.comment_star = e == b'*';
                },
                CSSStep::String => {
                    self.wrote(e);

                    if self.escape == 0 || !self.continue_escape(e) {
                        if e == b'\\' {
                            self.escape = 1;
                        } else if e == self.quote || matches!(e, b'\n' | b'\r' | 0x0C) {
                            // A newline ends a bad string, and the newline stays.
                            self.step = CSSStep::Normal;
                        }
                    }
                },
                CSSStep::Escape => {
                    self.wrote(e);

                    if matches!(e, b'\n' | b'\r' | 0x0C) {
                        // A backslash before a newline is not an escape, and the newline stays.
                        self.in_name = false;
                        self.url_matched = 0;
                        self.step = CSSStep::Normal;
                    } else if e.is_ascii_hexdigit() {
                        self.hex_digits = 1;
                        self.hex_value = hex_digit_value(e);
                        self.step = CSSStep::HexEscape;
                    } else {
                        self.name_char(u32::from(e));
                        self.last_escape = true;
                        self.step = CSSStep::Normal;
                    }
                },
                CSSStep::HexEscape => {
                    if self.hex_digits < 6 && e.is_ascii_hexdigit() {
                        self.wrote(e);
                        self.hex_digits += 1;
                        self.hex_value = self.hex_value * 16 + hex_digit_value(e);
                    } else {
                        let terminator = is_whitespace(e);

                        // The whitespace after a hex escape is a part of the escape.
                        if terminator {
                            self.wrote(e);
                        }

                        self.name_char(self.hex_value);
                        self.last_escape = true;
                        self.open_hex = !terminator;
                        self.step = CSSStep::Normal;

                        if !terminator {
                            continue;
                        }
                    }
                },
                CSSStep::URLStart => {
                    if is_whitespace(e) {
                        // The whitespace at the start of `url()` is not a part of the URL.
                        if run < p {
                            out.push_bytes(&bytes[run..p])?;
                        }

                        run = p + 1;
                    } else {
                        // A quoted URL is an ordinary string argument.
                        self.step = if matches!(e, b'"' | b'\'' | b')') {
                            CSSStep::Normal
                        } else {
                            self.escape = 0;
                            CSSStep::URLBody
                        };

                        continue;
                    }
                },
                CSSStep::URLBody => {
                    self.wrote(e);

                    if self.escape == 0 || !self.continue_escape(e) {
                        if e == b'\\' {
                            self.escape = 1;
                        } else if e == b')' {
                            self.step = CSSStep::Normal;
                        }
                    }
                },
                CSSStep::CustomStart => {
                    if is_whitespace(e) {
                        // The whitespace at the start of a custom property value is not a part of the value.
                        if run < p {
                            out.push_bytes(&bytes[run..p])?;
                        }

                        run = p + 1;
                        self.custom_space = true;
                    } else {
                        // Old browsers drop an empty value without whitespace, such as `--x:;`, so one space stays.
                        if self.custom_space && matches!(e, b';' | b'}' | b'!') {
                            out.push(b' ')?;
                            self.wrote(b' ');
                        }

                        self.custom_depth = 0;
                        self.custom_quote = 0;
                        self.custom_comment = 0;
                        self.escape = 0;
                        self.step = CSSStep::Custom;

                        continue;
                    }
                },
                CSSStep::Custom => {
                    // A custom property value is kept as it is until it ends at the top level.
                    if self.custom_comment >= 2 {
                        self.custom_comment = match e {
                            b'*' => 3,
                            b'/' if self.custom_comment == 3 => 0,
                            _ => 2,
                        };
                    } else if self.escape > 0 && self.continue_escape(e) {
                        // The byte belongs to an escape.
                    } else if self.custom_quote != 0 {
                        if e == b'\\' {
                            self.escape = 1;
                        } else if e == self.custom_quote || matches!(e, b'\n' | b'\r' | 0x0C) {
                            self.custom_quote = 0;
                        }
                    } else if self.custom_comment == 1 && e == b'*' {
                        self.custom_comment = 2;
                    } else {
                        self.custom_comment = 0;

                        match e {
                            b'/' => self.custom_comment = 1,
                            b'\\' => self.escape = 1,
                            b'"' | b'\'' => self.custom_quote = e,
                            b'(' | b'[' | b'{' => {
                                self.custom_depth = self.custom_depth.saturating_add(1)
                            },
                            b')' | b']' => self.custom_depth = self.custom_depth.saturating_sub(1),
                            b';' | b'}' if self.custom_depth == 0 => {
                                self.step = CSSStep::Normal;

                                continue;
                            },
                            b'}' => self.custom_depth -= 1,
                            _ => (),
                        }
                    }

                    self.wrote(e);
                },
            }

            p += 1;
        }

        if run < bytes.len() {
            out.push_bytes(&bytes[run..])?;
        }

        Ok(())
    }

    /// Write the rest of the minified CSS.
    pub(crate) fn finish(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        if self.step == CSSStep::Slash {
            // A `/` at the end is a delimiter.
            if self.gap {
                self.write_gap(b'/', out)?;
            }

            out.push(b'/')?;
        }

        // Whitespace and an unterminated comment at the end are removed.
        self.reset();

        Ok(())
    }
}
