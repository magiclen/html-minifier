use std::{fmt::Write as _, str::from_utf8};

use educe::Educe;

use crate::{HTMLMinifierError, HTMLWriter, functions::*};

/// Minify the JavaScript content of a `script` element, which arrives in chunks.
#[derive(Educe, Clone)]
#[educe(Debug, Default)]
pub(crate) struct JSMinifier {
    #[educe(Debug(method = "str_bytes_fmt"))]
    buffer:   Vec<u8>,
    #[educe(Debug(ignore))]
    minified: String,
}

impl JSMinifier {
    /// Prepare for a new script.
    #[inline]
    pub(crate) fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Input a chunk of JavaScript.
    #[inline]
    pub(crate) fn feed(
        &mut self,
        bytes: &[u8],
        _out: &mut impl HTMLWriter,
    ) -> Result<(), HTMLMinifierError> {
        self.buffer.extend_from_slice(bytes);

        Ok(())
    }

    /// Write the rest of the minified JavaScript.
    pub(crate) fn finish(&mut self, out: &mut impl HTMLWriter) -> Result<(), HTMLMinifierError> {
        let text = from_utf8(&self.buffer)?;

        self.minified.clear();
        write!(
            self.minified,
            "{}",
            minifier::js::minify(text).map_err(HTMLMinifierError::JSError)?
        )
        .expect("Writing to a String cannot fail.");

        out.push_bytes(self.minified.as_bytes())?;
        self.buffer.clear();

        Ok(())
    }
}
