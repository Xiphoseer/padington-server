use displaydoc::Display;
use http::{header::ToStrError, Response as HttpResponse};
use std::{io, result::Result as StdResult};
use thiserror::Error;
use tungstenite::http;

#[derive(Debug, Error, Display)]
pub(crate) enum Error {
    /// IO Error: {0}
    Io(#[from] io::Error),
    /// Invalid Header Value: {0}
    ToStr(#[from] ToStrError),
}

pub(crate) type Result<T> = StdResult<T, Error>;

// Assumes that this is a valid response
pub(crate) fn write_response<T>(mut w: impl io::Write, response: &HttpResponse<T>) -> Result<()> {
    writeln!(w, "{:?} {}\r", response.version(), response.status())?;

    for (k, v) in response.headers() {
        writeln!(w, "{}: {}\r", k, v.to_str()?).unwrap();
    }

    writeln!(w, "\r")?;

    Ok(())
}
