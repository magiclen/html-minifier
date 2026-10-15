use std::mem::take;

use educe::Educe;

use crate::{HTMLMinifierError, HTMLWriter, comment::HeldComment, functions::*};

/// The deepest nesting of template literals that is followed. Deeper code is kept as it is.
const MAX_TEMPLATE_DEPTH: usize = 256;

/// The most content that is held after `<!--` while looking for `<script`.
const MAX_DEFERRED_LENGTH: usize = 256 * 1024;

/// Keywords after which a `/` starts a regular expression.
const REGEX_KEYWORDS: [&[u8]; 14] = [
    b"return",
    b"typeof",
    b"instanceof",
    b"in",
    b"of",
    b"new",
    b"delete",
    b"void",
    b"throw",
    b"case",
    b"do",
    b"else",
    b"yield",
    b"await",
];

#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum JSStep {
    #[educe(Default)]
    Normal,
    Slash,
    LineComment,
    BlockComment,
    KeptBlockComment,
    String,
    Template,
    Regex,
    RegexFlags,
    /// A comment line that is kept, such as `<!--` in a classic script.
    LineKept,
    Verbatim,
}

/// The kind of the latest token, which decides how a `/` and a line break after it work.
#[derive(Educe, Debug, Copy, Clone, Eq, PartialEq)]
#[educe(Default)]
enum Token {
    #[educe(Default)]
    None,
    /// An identifier or a keyword that a division can follow.
    Word,
    /// A keyword that a regular expression can follow, such as `return`.
    RegexKeyword,
    Number,
    /// A punctuator that continues an expression, such as `=` or `(`.
    Punctuator,
    /// `++` or `--`.
    IncDec,
    /// `)` that a division can follow.
    CloseParen,
    /// `)` of `if`, `while`, `for` or `with`, which a regular expression can follow.
    ControlParen,
    CloseBracket,
    CloseBrace,
    /// A string or a template literal.
    Literal,
    Regex,
}

/// Check whether a byte can be a part of an identifier, a keyword or a number.
#[inline]
const fn is_word_byte(e: u8) -> bool {
    e.is_ascii_alphanumeric() || matches!(e, b'_' | b'$' | b'\\') || e >= 0x80
}

/// Minify the JavaScript content of a `script` element, which arrives in chunks.
/// Only comments and whitespace are removed, and line breaks that can change automatic semicolon insertion stay.
#[derive(Educe, Clone)]
#[educe(Debug, Default)]
pub(crate) struct JSMinifier {
    step:                     JSStep,
    module:                   bool,
    /// The last two written bytes, with the latest one at the end. 0 means nothing.
    last:                     [u8; 2],
    token:                    Token,
    /// Whether the latest punctuator is `.`, which makes the next word a property name.
    after_dot:                bool,
    /// Whether the latest word is `if`, `while`, `for`, `with` or the `await` of `for await`.
    control_word:             bool,
    /// Whether the latest word is `for`.
    after_for:                bool,
    /// Whether whitespace or comments are pending between two tokens.
    gap:                      bool,
    /// Whether the pending gap includes a line break.
    gap_newline:              bool,
    /// Whether only whitespace and comments are after the latest line break.
    line_start:               bool,
    word:                     [u8; 10],
    /// The length of the current word, up to 255.
    word_length:              u8,
    word_after_dot:           bool,
    /// One bit for each open parenthesis, which is set for `if (`, `while (`, `for (` and `with (`.
    parens:                   u64,
    paren_depth:              u32,
    brace_depth:              u32,
    /// The brace depths where template substitutions start.
    templates:                Vec<u32>,
    quote:                    u8,
    escape:                   bool,
    regex_class:              bool,
    template_dollar:          bool,
    comment_star:             bool,
    comment_newline:          bool,
    /// How many bytes of U+2028 or U+2029 have been matched.
    separator:                u8,
    /// How many bytes of `<!--` have been matched as tokens, in a classic script.
    html_open:                u8,
    /// How many bytes of `-->` have been matched as tokens at the start of a line, in a classic script.
    html_close:               u8,
    /// Whether the script starts with `#`, which can start `#!`.
    hashbang:                 bool,
    /// The token state before a comment that is written as tokens, such as `<!--`.
    saved_token:              (Token, bool),
    /// How many bytes of `<!--` have been matched in the input.
    raw_open:                 u8,
    /// How many bytes of `<script` have been matched in the input after `<!--`.
    raw_script:               u8,
    /// Whether `<!--` has been found in the input.
    escaped:                  bool,
    /// Whether the input after `<!--` is being held.
    deferring:                bool,
    /// Whether `<!--` is in a line comment, whose start has been removed.
    deferred_in_line_comment: bool,
    #[educe(Debug(method = "str_bytes_fmt"))]
    deferred:                 Vec<u8>,
    comment:                  HeldComment,
}

impl JSMinifier {
    /// Prepare for a new script.
    #[inline]
    pub(crate) fn reset(&mut self, module: bool) {
        let mut templates = take(&mut self.templates);
        let mut deferred = take(&mut self.deferred);
        let comment = take(&mut self.comment);

        templates.clear();
        deferred.clear();

        *self = Self {
            module,
            line_start: true,
            templates,
            deferred,
            comment,
            ..Self::default()
        };
    }

    #[inline]
    fn wrote(&mut self, e: u8) {
        self.last = [self.last[1], e];
    }

    /// Check whether the latest written token and a token starting with `next` become different tokens without a space between them.
    fn merges(&self, next: u8) -> bool {
        let [before, last] = self.last;

        (is_word_byte(last) && is_word_byte(next))
            || (self.token == Token::Regex && is_word_byte(next))
            || (self.token == Token::Number && next == b'.')
            || (matches!(last, b'+' | b'-' | b'/') && next == last)
            || (last == b'/' && next == b'*')
            // `</`, `<!--` and `<script` affect where browsers end a `script` element.
            || (last == b'<' && matches!(next, b'!' | b'/' | b's' | b'S'))
            || (before == b'<' && last == b'!' && next == b'-')
            || (before == b'-' && last == b'-' && next == b'>')
    }

    /// Check whether a line break between the latest token and a token starting with `next` can be removed without changing automatic semicolon insertion.
    fn removes_newline(&self, next: u8) -> bool {
        // A line starting with `-->` is a comment in a classic script.
        if !self.module && next == b'-' {
            return false;
        }

        self.token == Token::Punctuator
            || matches!(
                next,
                b')' | b']'
                    | b'}'
                    | b','
                    | b';'
                    | b':'
                    | b'?'
                    | b'='
                    | b'%'
                    | b'<'
                    | b'>'
                    | b'&'
                    | b'|'
                    | b'^'
            )
    }

    /// Write what replaces the pending whitespace and comments before a token starting with `next`.
    fn write_gap(&mut self, next: u8, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        // Nothing is needed at the start of the script.
        if self.last[1] != 0 {
            if self.gap_newline && !self.removes_newline(next) {
                out.push(b'\n')?;
                self.wrote(b'\n');
            } else if self.merges(next) {
                out.push(b' ')?;
                self.wrote(b' ');
            }
        }

        self.gap = false;
        self.gap_newline = false;

        Ok(())
    }

    /// Write the held comment, which has to be kept.
    fn keep_comment(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        // The next token is not known yet, so a pending line break always stays.
        if self.gap && self.last[1] != 0 {
            if self.gap_newline {
                out.push(b'\n')?;
                self.wrote(b'\n');
            } else if self.merges(b'/') {
                out.push(b' ')?;
                self.wrote(b' ');
            }
        }

        self.gap = false;
        self.gap_newline = false;

        let held = self.comment.as_bytes();
        out.push_bytes(held)?;
        self.last = [held[held.len() - 2], held[held.len() - 1]];

        self.step = JSStep::KeptBlockComment;

        Ok(())
    }

    /// Check whether a byte completes U+2028 or U+2029, which are line breaks.
    #[inline]
    fn completes_separator(&mut self, e: u8) -> bool {
        self.separator = match (self.separator, e) {
            (_, 0xE2) => 1,
            (1, 0x80) => 2,
            (2, 0xA8 | 0xA9) => {
                self.separator = 0;

                return true;
            },
            _ => 0,
        };

        false
    }

    #[inline]
    fn regex_allowed(&self) -> bool {
        matches!(
            self.token,
            Token::None
                | Token::Punctuator
                | Token::RegexKeyword
                | Token::ControlParen
                | Token::CloseBrace
        )
    }

    /// Decide the kind of the current word, which has ended.
    fn end_word(&mut self) {
        if self.word_length == 0 {
            return;
        }

        let keyword: &[u8] = if self.word_length <= 10 && !self.word_after_dot {
            &self.word[..usize::from(self.word_length)]
        } else {
            &[]
        };

        self.token = if self.word[0].is_ascii_digit() {
            Token::Number
        } else if REGEX_KEYWORDS.contains(&keyword) {
            Token::RegexKeyword
        } else {
            Token::Word
        };
        self.control_word = matches!(keyword, b"if" | b"while" | b"for" | b"with")
            || (keyword == b"await" && self.after_for);
        self.after_for = keyword == b"for";
        self.word_length = 0;
    }

    /// Find `#!` at the start, and `<!--` and `-->` at the start of a line in a classic script.
    /// They start comment lines that are kept.
    fn starts_line_comment(&mut self, e: u8, adjacent: bool, first: bool) -> bool {
        let saved = (self.token, self.after_dot);

        if self.hashbang {
            self.hashbang = false;

            if adjacent && e == b'!' {
                return true;
            }
        }

        if first && e == b'#' {
            self.saved_token = saved;
            self.hashbang = true;

            return false;
        }

        if self.module {
            return false;
        }

        self.html_open = match (self.html_open, e) {
            (_, b'<') => {
                self.saved_token = saved;
                1
            },
            (1, b'!') if adjacent => 2,
            (2, b'-') if adjacent => 3,
            (3, b'-') if adjacent => 4,
            _ => 0,
        };

        self.html_close = match (self.html_close, e) {
            (0, b'-') if self.line_start => {
                self.saved_token = saved;
                1
            },
            (1, b'-') if adjacent => 2,
            (2, b'>') if adjacent => 3,
            _ => 0,
        };

        if self.html_open == 4 || self.html_close == 3 {
            self.html_open = 0;
            self.html_close = 0;

            true
        } else {
            false
        }
    }

    #[inline]
    fn push_paren(&mut self, control: bool) {
        if self.paren_depth < 64 {
            let bit = 1 << self.paren_depth;

            self.parens = if control { self.parens | bit } else { self.parens & !bit };
        }

        self.paren_depth = self.paren_depth.saturating_add(1);
    }

    #[inline]
    fn pop_paren(&mut self) -> bool {
        if self.paren_depth == 0 {
            return false;
        }

        self.paren_depth -= 1;

        self.paren_depth < 64 && self.parens & (1 << self.paren_depth) != 0
    }

    /// Track a punctuator, which has been written.
    fn punctuator(&mut self, e: u8, adjacent: bool) {
        let control = self.control_word;

        self.control_word = false;
        self.after_for = false;
        self.after_dot = e == b'.';

        self.token = match e {
            b'"' | b'\'' => {
                self.quote = e;
                self.escape = false;
                self.step = JSStep::String;

                return;
            },
            b'`' => {
                self.escape = false;
                self.template_dollar = false;
                self.step = JSStep::Template;

                return;
            },
            b'(' => {
                self.push_paren(control);

                Token::Punctuator
            },
            b')' => {
                if self.pop_paren() {
                    Token::ControlParen
                } else {
                    Token::CloseParen
                }
            },
            b']' => Token::CloseBracket,
            b'{' => {
                self.brace_depth = self.brace_depth.saturating_add(1);

                Token::Punctuator
            },
            b'}' => {
                if self.templates.last() == Some(&self.brace_depth) {
                    // The substitution of a template literal ends.
                    self.templates.pop();
                    self.escape = false;
                    self.template_dollar = false;
                    self.step = JSStep::Template;

                    return;
                }

                self.brace_depth = self.brace_depth.saturating_sub(1);

                Token::CloseBrace
            },
            b'+' | b'-' if adjacent && self.last[0] == e => Token::IncDec,
            _ => Token::Punctuator,
        };
    }

    /// Input a chunk of JavaScript.
    pub(crate) fn feed(
        &mut self,
        bytes: &[u8],
        out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        if self.step == JSStep::Verbatim {
            return out.push_bytes(bytes);
        }

        if self.deferring {
            return self.defer(bytes, out);
        }

        // The start of the bytes that are written as they are.
        let mut run = 0;
        let mut p = 0;

        while p < bytes.len() {
            let e = bytes[p];

            match self.step {
                JSStep::Normal => {
                    if is_whitespace(e) || e == b'/' {
                        if run < p {
                            out.push_bytes(&bytes[run..p])?;
                        }

                        run = p + 1;
                        self.end_word();
                        self.html_open = 0;
                        self.html_close = 0;
                        self.hashbang = false;

                        if e == b'/' {
                            self.step = JSStep::Slash;
                        } else {
                            self.gap = true;

                            if matches!(e, b'\n' | b'\r') {
                                self.gap_newline = true;
                                self.line_start = true;
                            }
                        }
                    } else if is_word_byte(e) {
                        if self.word_length == 0 {
                            if self.gap {
                                self.write_gap(e, out)?;
                            }

                            self.word_after_dot = self.after_dot;
                            self.after_dot = false;
                            self.word[0] = e;
                        } else if self.word_length < 10 {
                            self.word[usize::from(self.word_length)] = e;
                        }

                        self.word_length = self.word_length.saturating_add(1);
                        self.wrote(e);
                        self.line_start = false;
                        self.html_open = 0;
                        self.html_close = 0;
                        self.hashbang = false;
                    } else {
                        self.end_word();

                        let adjacent = !self.gap;
                        let first = self.last[1] == 0 && !self.gap;

                        if self.gap {
                            self.write_gap(e, out)?;
                        }

                        self.wrote(e);

                        if self.starts_line_comment(e, adjacent, first) {
                            // The comment is not a token, so the token state before it comes back.
                            (self.token, self.after_dot) = self.saved_token;
                            self.separator = 0;
                            self.step = JSStep::LineKept;
                        } else {
                            self.punctuator(e, adjacent);
                        }

                        self.line_start = false;
                    }
                },
                JSStep::Slash => match e {
                    b'/' => {
                        run = p + 1;
                        self.gap = true;
                        self.separator = 0;
                        self.step = JSStep::LineComment;
                    },
                    b'*' => {
                        run = p + 1;
                        self.gap = true;
                        self.comment.start();
                        self.comment_star = false;
                        self.comment_newline = false;
                        self.separator = 0;
                        self.step = JSStep::BlockComment;
                    },
                    _ => {
                        // The `/` is a division or the start of a regular expression.
                        let regex = self.regex_allowed();

                        if self.gap {
                            self.write_gap(b'/', out)?;
                        }

                        out.push(b'/')?;
                        self.wrote(b'/');
                        self.line_start = false;
                        self.control_word = false;
                        self.after_for = false;
                        self.after_dot = false;

                        if regex {
                            self.escape = false;
                            self.regex_class = false;
                            self.step = JSStep::Regex;
                        } else {
                            self.token = Token::Punctuator;
                            self.step = JSStep::Normal;
                        }

                        continue;
                    },
                },
                JSStep::LineComment => {
                    run = p + 1;

                    if matches!(e, b'\n' | b'\r') {
                        // The line break is a part of the gap.
                        self.step = JSStep::Normal;

                        continue;
                    }

                    if self.completes_separator(e) {
                        self.gap_newline = true;
                        self.line_start = true;
                        self.step = JSStep::Normal;
                    }
                },
                JSStep::BlockComment => {
                    run = p + 1;

                    if self.comment_star && e == b'/' {
                        // The comment is removed, and a comment with a line break works as a line break.
                        if self.comment_newline {
                            self.gap_newline = true;
                            self.line_start = true;
                        }

                        self.step = JSStep::Normal;
                    } else {
                        if matches!(e, b'\n' | b'\r') || self.completes_separator(e) {
                            self.comment_newline = true;
                        }

                        if self.comment.push(e) {
                            self.keep_comment(out)?;
                        }
                    }

                    self.comment_star = e == b'*';
                },
                JSStep::KeptBlockComment => {
                    self.wrote(e);

                    if self.comment_star && e == b'/' {
                        // The token before the comment stays the latest token.
                        self.line_start = self.comment_newline;
                        self.step = JSStep::Normal;
                    } else if matches!(e, b'\n' | b'\r') {
                        self.comment_newline = true;
                    }

                    self.comment_star = e == b'*';
                },
                JSStep::String => {
                    if !self.escape && matches!(e, b'\n' | b'\r') {
                        // An unterminated string stays as it is.
                        self.token = Token::Literal;
                        self.step = JSStep::Normal;

                        continue;
                    }

                    self.wrote(e);

                    if self.escape {
                        self.escape = false;
                    } else if e == b'\\' {
                        self.escape = true;
                    } else if e == self.quote {
                        self.token = Token::Literal;
                        self.step = JSStep::Normal;
                    }
                },
                JSStep::Template => {
                    let escaped = self.escape;

                    self.wrote(e);
                    self.escape = !escaped && e == b'\\';

                    if !escaped {
                        if e == b'`' {
                            self.token = Token::Literal;
                            self.step = JSStep::Normal;
                        } else if self.template_dollar && e == b'{' {
                            if self.templates.len() >= MAX_TEMPLATE_DEPTH {
                                // Such deep nesting is kept as it is.
                                self.step = JSStep::Verbatim;
                            } else {
                                self.templates.push(self.brace_depth);
                                self.token = Token::Punctuator;
                                self.step = JSStep::Normal;
                            }
                        }
                    }

                    self.template_dollar = !escaped && e == b'$';
                },
                JSStep::Regex => {
                    if matches!(e, b'\n' | b'\r') {
                        // An unterminated regular expression stays as it is.
                        self.token = Token::Regex;
                        self.step = JSStep::Normal;

                        continue;
                    }

                    self.wrote(e);

                    if self.escape {
                        self.escape = false;
                    } else if e == b'\\' {
                        self.escape = true;
                    } else if self.regex_class {
                        self.regex_class = e != b']';
                    } else if e == b'[' {
                        self.regex_class = true;
                    } else if e == b'/' {
                        self.step = JSStep::RegexFlags;
                    }
                },
                JSStep::RegexFlags => {
                    if is_word_byte(e) {
                        self.wrote(e);
                    } else {
                        self.token = Token::Regex;
                        self.step = JSStep::Normal;

                        continue;
                    }
                },
                JSStep::LineKept => {
                    self.wrote(e);

                    if matches!(e, b'\n' | b'\r') || self.completes_separator(e) {
                        // The line break after the comment is written as it is.
                        self.line_start = true;
                        self.step = JSStep::Normal;
                    }
                },
                JSStep::Verbatim => (),
            }

            if !self.escaped && self.step != JSStep::Verbatim {
                self.raw_open = match (self.raw_open, e) {
                    (_, b'<') => 1,
                    (1, b'!') => 2,
                    (2, b'-') => 3,
                    (3, b'-') => 4,
                    _ => 0,
                };

                if self.raw_open == 4 {
                    // `<!--` can change how browsers end the `script` element.
                    self.escaped = true;

                    if self.step == JSStep::BlockComment {
                        self.keep_comment(out)?;
                    }

                    if run <= p {
                        out.push_bytes(&bytes[run..=p])?;
                    }

                    self.deferring = true;
                    self.deferred_in_line_comment = self.step == JSStep::LineComment;

                    return self.defer(&bytes[p + 1..], out);
                }
            }

            p += 1;
        }

        if run < bytes.len() {
            out.push_bytes(&bytes[run..])?;
        }

        Ok(())
    }

    /// Hold the input after `<!--` until it is clear whether `<script` follows.
    fn defer(&mut self, bytes: &[u8], out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        for (p, &e) in bytes.iter().enumerate() {
            let e = e.to_ascii_lowercase();

            self.raw_script = match self.raw_script {
                _ if e == b'<' => 1,
                n @ 1..=6 if e == b"<script"[usize::from(n)] => n + 1,
                _ => 0,
            };

            if self.raw_script == 7 {
                // After `<!--`, `<script` can make browsers ignore the next `</script>`, so everything from `<!--` is kept.
                self.deferred.extend_from_slice(&bytes[..=p]);
                self.write_deferred(out)?;

                return out.push_bytes(&bytes[p + 1..]);
            }
        }

        self.deferred.extend_from_slice(bytes);

        if self.deferred.len() > MAX_DEFERRED_LENGTH {
            self.write_deferred(out)?;
        }

        Ok(())
    }

    /// Write the held input as it is, and keep the rest of the script as it is.
    fn write_deferred(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        if self.deferred_in_line_comment {
            // The start of the line comment has been removed, so the comment starts again.
            out.push_bytes(b"//<!--")?;
        }

        out.push_bytes(&self.deferred)?;
        self.deferred.clear();
        self.deferring = false;
        self.step = JSStep::Verbatim;

        Ok(())
    }

    /// Write the rest of the minified JavaScript.
    pub(crate) fn finish(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        if self.deferring {
            // `<script` does not follow `<!--`, so the held input is minified as usual.
            self.deferring = false;

            let deferred = take(&mut self.deferred);

            self.feed(&deferred, out)?;
            self.deferred = deferred;
        }

        match self.step {
            JSStep::Slash => {
                // A `/` at the end is a division or an unterminated regular expression.
                if self.gap {
                    self.write_gap(b'/', out)?;
                }

                out.push(b'/')?;
            },
            JSStep::BlockComment => {
                // An unterminated comment is a syntax error, so it is kept to keep the error.
                self.keep_comment(out)?;
            },
            _ => (),
        }

        // Whitespace at the end is removed.
        self.reset(self.module);

        Ok(())
    }
}
