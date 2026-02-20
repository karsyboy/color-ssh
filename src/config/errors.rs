//! Configuration-related error types

use std::{error::Error, fmt, io};

/// Errors that can occur during configuration loading and processing
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error when reading config files
    IoError(io::Error),
    /// Configuration has already been initialized
    AlreadyInitialized,
}

impl fmt::Display for ConfigError {
    // User-facing error formatting.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(err) => write!(f, "I/O error: {}", err),
            ConfigError::AlreadyInitialized => write!(f, "Configuration has already been initialized"),
        }
    }
}

// Marker trait for `std::error::Error` compatibility.
impl Error for ConfigError {}

// Convert I/O errors into config errors.
impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        ConfigError::IoError(err)
    }
}
