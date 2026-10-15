use crate::functions::{is_end_tag, is_whitespace};

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
enum ScriptStep {
    #[default]
    Data,
    LessThan,
    EscapeStart,
    EscapeStartDash,
    Escaped,
    EscapedDash,
    EscapedDashDash,
    EscapedLessThan,
    DoubleEscapeStart,
    DoubleEscaped,
    DoubleEscapedDash,
    DoubleEscapedDashDash,
    DoubleEscapedLessThan,
    DoubleEscapeEnd,
}

/// Follow the HTML script states that can hide an end tag.
#[derive(Debug, Default, Clone)]
pub(crate) struct ScriptData {
    step: ScriptStep,
    word: u8,
}

impl ScriptData {
    #[inline]
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub(crate) fn can_skip(&self) -> bool {
        matches!(self.step, ScriptStep::Data | ScriptStep::Escaped | ScriptStep::DoubleEscaped)
    }

    #[inline]
    pub(crate) fn tracks_dash(&self) -> bool {
        self.step != ScriptStep::Data
    }

    #[inline]
    pub(crate) fn ends(&mut self, e: u8, end_tag: &[u8], counter: &mut u8) -> bool {
        if end_tag == b"</script" && !self.feed(e) {
            *counter = 0;
            return false;
        }

        is_end_tag(e, end_tag, counter)
    }

    /// Return whether this byte can be part of a real end tag.
    fn feed(&mut self, e: u8) -> bool {
        use ScriptStep::*;

        let double = matches!(
            self.step,
            DoubleEscaped
                | DoubleEscapedDash
                | DoubleEscapedDashDash
                | DoubleEscapedLessThan
                | DoubleEscapeEnd
        );

        self.step = match self.step {
            Data | LessThan => match e {
                b'<' => LessThan,
                b'!' if self.step == LessThan => EscapeStart,
                _ => Data,
            },
            EscapeStart => {
                if e == b'-' {
                    EscapeStartDash
                } else if e == b'<' {
                    LessThan
                } else {
                    Data
                }
            },
            EscapeStartDash => {
                if e == b'-' {
                    EscapedDashDash
                } else if e == b'<' {
                    LessThan
                } else {
                    Data
                }
            },
            Escaped | EscapedDash | EscapedDashDash => match e {
                b'<' => EscapedLessThan,
                b'-' if self.step == Escaped => EscapedDash,
                b'-' => EscapedDashDash,
                b'>' if self.step == EscapedDashDash => Data,
                _ => Escaped,
            },
            EscapedLessThan => {
                if e.is_ascii_alphabetic() {
                    self.word = if e.eq_ignore_ascii_case(&b's') { 1 } else { 255 };
                    DoubleEscapeStart
                } else if e == b'<' {
                    EscapedLessThan
                } else if e == b'-' {
                    EscapedDash
                } else {
                    Escaped
                }
            },
            DoubleEscapeStart | DoubleEscapeEnd => {
                let entering = self.step == DoubleEscapeStart;
                if e.is_ascii_alphabetic() {
                    self.word = if self.word < 6
                        && e.eq_ignore_ascii_case(&b"script"[usize::from(self.word)])
                    {
                        self.word + 1
                    } else {
                        255
                    };
                    self.step
                } else if is_whitespace(e) || matches!(e, b'/' | b'>') {
                    if (self.word == 6) == entering { DoubleEscaped } else { Escaped }
                } else if entering {
                    match e {
                        b'<' => EscapedLessThan,
                        b'-' => EscapedDash,
                        _ => Escaped,
                    }
                } else {
                    match e {
                        b'<' => DoubleEscapedLessThan,
                        b'-' => DoubleEscapedDash,
                        _ => DoubleEscaped,
                    }
                }
            },
            DoubleEscaped | DoubleEscapedDash | DoubleEscapedDashDash => match e {
                b'<' => DoubleEscapedLessThan,
                b'-' if self.step == DoubleEscaped => DoubleEscapedDash,
                b'-' => DoubleEscapedDashDash,
                b'>' if self.step == DoubleEscapedDashDash => Data,
                _ => DoubleEscaped,
            },
            DoubleEscapedLessThan => {
                if e == b'/' {
                    self.word = 0;
                    DoubleEscapeEnd
                } else {
                    match e {
                        b'<' => DoubleEscapedLessThan,
                        b'-' => DoubleEscapedDash,
                        _ => DoubleEscaped,
                    }
                }
            },
        };

        !double
    }
}
