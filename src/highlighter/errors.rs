//! Syntax highlighting error types

use std::{error::Error, fmt, io};

/// Errors that can occur during syntax highlighting
#[derive(Debug)]
pub enum HighlightError {
    /// I/O error during highlighting operations
    IoError(io::Error),
    /// Failed to create required directories
    DirectoryCreationError(String),
    /// Error formatting highlighted output
    FormattingError(String),
}

impl fmt::Display for HighlightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HighlightError::IoError(err) => write!(f, "I/O error: {}", err),
            HighlightError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            HighlightError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for HighlightError {}

impl From<io::Error> for HighlightError {
    fn from(err: io::Error) -> Self {
        HighlightError::IoError(err)
    }
}
