use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
};

/// Errors for `HTMLMinifier`.
#[derive(Debug)]
pub enum HTMLMinifierError {
    CSSError(&'static str),
    IOError(io::Error),
}

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
            HTMLMinifierError::IOError(error) => Display::fmt(error, f),
        }
    }
}

impl Error for HTMLMinifierError {}
