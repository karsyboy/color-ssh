use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum ConfigError {
    IoError(io::Error),
    DirectoryCreationError(String),
    FormattingError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(err) => write!(f, "I/O error: {}", err),
            ConfigError::DirectoryCreationError(msg) => {
                write!(f, "Failed to create directory: {}", msg)
            }
            ConfigError::FormattingError(msg) => write!(f, "Formatting error: {}", msg),
        }
    }
}

impl Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        ConfigError::IoError(err)
    }
}
