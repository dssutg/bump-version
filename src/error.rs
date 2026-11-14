//! The shared error type of the program. Almost every failure is described
//! by its message; the typed variants exist for errors callers legitimately
//! react to.

use std::fmt;
use std::io;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type of the whole program.
#[derive(Debug)]
pub enum Error {
    /// The release window contains commits that fail Conventional Commits
    /// validation while invalid commits are not ignored.
    InvalidCommits,
    /// An I/O failure, kept typed so callers can react to specific kinds
    /// (for example a missing config file).
    Io(io::Error),
    /// Any other fatal error, described by its message.
    Message(String),
}

impl Error {
    /// Builds a message error from anything printable.
    pub fn msg(text: impl fmt::Display) -> Error {
        Error::Message(text.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidCommits => write!(f, "found invalid commits"),
            Error::Io(err) => write!(f, "{err}"),
            Error::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}
