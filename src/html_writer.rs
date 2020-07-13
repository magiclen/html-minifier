#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::io::Write;

use crate::HTMLMinifierError;

/// Implement this trait to build a HTML writer.
pub trait HTMLWriter {
    fn push(&mut self, e: u8) -> Result<(), HTMLMinifierError>;
    fn push_bytes(&mut self, bytes: &[u8]) -> Result<(), HTMLMinifierError>;
}

#[cfg(not(feature = "std"))]
impl HTMLWriter for Vec<u8> {
    #[inline]
    fn push(&mut self, e: u8) -> Result<(), HTMLMinifierError> {
        self.push(e);

        Ok(())
    }

    #[inline]
    fn push_bytes(&mut self, bytes: &[u8]) -> Result<(), HTMLMinifierError> {
        self.extend_from_slice(bytes);

        Ok(())
    }
}

#[cfg(feature = "std")]
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
