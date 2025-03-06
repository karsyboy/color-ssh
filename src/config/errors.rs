use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum ConfigError {
    IoError(io::Error),
    DirectoryCreationError(String),
    FormattingError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(e) => write!(f, "I/O error: {}", e),
            ConfigError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            ConfigError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        ConfigError::IoError(error)
    }
}
