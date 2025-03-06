use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum UIError {
    IoError(io::Error),
    DirectoryCreationError(String),
    FormattingError(String),
}

impl fmt::Display for UIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UIError::IoError(e) => write!(f, "I/O error: {}", e),
            UIError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            UIError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for UIError {}

impl From<io::Error> for UIError {
    fn from(error: io::Error) -> Self {
        UIError::IoError(error)
    }
}
