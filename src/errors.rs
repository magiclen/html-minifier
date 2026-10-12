use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    str::Utf8Error,
};

/// Errors for `HTMLMinifier`.
#[derive(Debug)]
pub enum HTMLMinifierError {
    JSError(&'static str),
    CSSError(&'static str),
    IOError(io::Error),
    UTF8Error(Utf8Error),
    IncompleteInput,
}

impl From<io::Error> for HTMLMinifierError {
    #[inline]
    fn from(error: io::Error) -> Self {
        HTMLMinifierError::IOError(error)
    }
}

impl From<Utf8Error> for HTMLMinifierError {
    #[inline]
    fn from(error: Utf8Error) -> Self {
        HTMLMinifierError::UTF8Error(error)
    }
}

impl Display for HTMLMinifierError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            HTMLMinifierError::JSError(error) => Display::fmt(error, f),
            HTMLMinifierError::CSSError(error) => Display::fmt(error, f),
            HTMLMinifierError::IOError(error) => Display::fmt(error, f),
            HTMLMinifierError::UTF8Error(error) => Display::fmt(error, f),
            HTMLMinifierError::IncompleteInput => f.write_str("The HTML input is incomplete."),
        }
    }
}

impl Error for HTMLMinifierError {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HTMLMinifierError::IOError(error) => Some(error),
            HTMLMinifierError::UTF8Error(error) => Some(error),
            _ => None,
        }
    }
}
