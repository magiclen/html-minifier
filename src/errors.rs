use core::fmt::{self, Display, Formatter};

#[cfg(feature = "std")]
use std::error::Error;

/// Errors for `HTMLMinifier`.
#[derive(Debug, Clone)]
pub enum HTMLMinifierError {
    CSSError(&'static str),
}

impl Display for HTMLMinifierError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            HTMLMinifierError::CSSError(error) => Display::fmt(error, f),
        }
    }
}

#[cfg(feature = "std")]
impl Error for HTMLMinifierError {}
