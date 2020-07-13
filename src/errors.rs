use core::fmt::{self, Display, Formatter};

#[cfg(feature = "std")]
use std::error::Error;

#[cfg(feature = "std")]
use std::io;

/// Errors for `HTMLMinifier`.
#[derive(Debug)]
pub enum HTMLMinifierError {
    CSSError(&'static str),
    #[cfg(feature = "std")]
    IOError(io::Error),
}

#[cfg(feature = "std")]
impl From<io::Error> for HTMLMinifierError {
    #[inline]
    fn from(error: io::Error) -> Self {
        HTMLMinifierError::IOError(error)
    }
}

impl Display for HTMLMinifierError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            HTMLMinifierError::CSSError(error) => Display::fmt(error, f),
            #[cfg(feature = "std")]
            HTMLMinifierError::IOError(error) => Display::fmt(error, f),
        }
    }
}

#[cfg(feature = "std")]
impl Error for HTMLMinifierError {}
