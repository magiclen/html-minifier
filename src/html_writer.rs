use std::io::Write;

use crate::HTMLMinifierError;

/// Implement this trait to build a HTML writer.
pub trait HTMLWriter {
    fn push(&mut self, e: u8) -> Result<(), HTMLMinifierError>;
    fn push_bytes(&mut self, bytes: &[u8]) -> Result<(), HTMLMinifierError>;
}

impl<W: Write> HTMLWriter for W {
    #[inline]
    fn push(&mut self, e: u8) -> Result<(), HTMLMinifierError> {
        Ok(self.write_all(&[e])?)
    }

    #[inline]
    fn push_bytes(&mut self, bytes: &[u8]) -> Result<(), HTMLMinifierError> {
        Ok(self.write_all(bytes)?)
    }
}
