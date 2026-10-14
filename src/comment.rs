use educe::Educe;

use crate::functions::*;

/// The longest comment that is held back. A longer comment is kept.
const MAX_HELD_LENGTH: usize = 64 * 1024;

/// Hold back a block comment of embedded code until it is clear whether the comment has to be kept.
#[derive(Educe, Clone)]
#[educe(Debug, Default)]
pub(crate) struct HeldComment {
    #[educe(Debug(method = "str_bytes_fmt"))]
    bytes:      Vec<u8>,
    /// Whether the text after `/*` has only `*` so far.
    stars_only: bool,
}

impl HeldComment {
    /// Start holding a comment with its `/*`.
    #[inline]
    pub(crate) fn start(&mut self) {
        self.bytes.clear();
        self.bytes.extend_from_slice(b"/*");
        self.stars_only = true;
    }

    /// Hold a byte of the comment.
    /// Return `true` when the comment has to be kept, like `/*!`, `@license` and `@preserve` comments in terser.
    #[inline]
    pub(crate) fn push(&mut self, e: u8) -> bool {
        self.bytes.push(e);

        if self.stars_only {
            if e == b'!' {
                return true;
            }

            self.stars_only = e == b'*';
        }

        ends_with_ignore_ascii_case(&self.bytes, b"@lic")
            || ends_with_ignore_ascii_case(&self.bytes, b"@preserve")
            || self.bytes.len() > MAX_HELD_LENGTH
    }

    #[inline]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
