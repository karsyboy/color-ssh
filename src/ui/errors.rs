use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum UIError {
    IoError(io::Error),
    DirectoryCreationError(String),
    FormattingError(String),
}

impl fmt::Display for UIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UIError::IoError(err) => write!(f, "I/O error: {}", err),
            UIError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            UIError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for UIError {}

impl From<io::Error> for UIError {
    fn from(err: io::Error) -> Self {
        UIError::IoError(err)
    }
}
