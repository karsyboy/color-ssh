use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum LogError {
    IoError(io::Error),
    DirectoryCreationError(String),
    FormattingError(String),
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::IoError(e) => write!(f, "I/O error: {}", e),
            LogError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            LogError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for LogError {}

impl From<io::Error> for LogError {
    fn from(error: io::Error) -> Self {
        LogError::IoError(error)
    }
}
