use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
};

/// Errors for `HTMLMinifier`.
#[derive(Debug)]
#[non_exhaustive]
pub enum HTMLMinifierError {
    IOError(io::Error),
    IncompleteInput,
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
            HTMLMinifierError::IOError(error) => Display::fmt(error, f),
            HTMLMinifierError::IncompleteInput => f.write_str("The HTML input is incomplete."),
        }
    }
}

impl Error for HTMLMinifierError {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            // The message of the I/O error is already shown, so the next error is its source.
            HTMLMinifierError::IOError(error) => error.source(),
            HTMLMinifierError::IncompleteInput => None,
        }
    }
}
