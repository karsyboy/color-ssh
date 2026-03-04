//! Configuration-related error types.

use std::{error::Error, fmt, io};

/// Errors returned by configuration loading and initialization.
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error while reading or writing config files.
    IoError(io::Error),
    /// Configuration was initialized more than once in an invalid context.
    AlreadyInitialized,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(err) => write!(f, "I/O error: {}", err),
            ConfigError::AlreadyInitialized => write!(f, "Configuration has already been initialized"),
        }
    }
}

impl Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        ConfigError::IoError(err)
    }
}
