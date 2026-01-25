//! Configuration-related error types

use std::{error::Error, fmt, io};

/// Errors that can occur during configuration loading and processing
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error when reading config files
    IoError(io::Error),
    /// Failed to create required directories
    DirectoryCreationError(String),
    /// Error formatting configuration data
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
