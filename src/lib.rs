pub mod args;
pub mod config;
pub mod highlighter;
pub mod log;
pub mod process;
pub mod ssh_config;
pub mod tui;

#[cfg(test)]
mod test;

use std::io;

/// Result type alias for color-ssh operations
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type encompassing all module-specific errors
#[derive(Debug)]
pub enum Error {
    /// I/O operation error
    Io(io::Error),
    /// Configuration loading or parsing error
    Config(config::ConfigError),
    /// Logging operation error
    Log(log::LogError),
}

impl std::fmt::Display for Error {
    // Human-readable top-level error formatting.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::Config(err) => write!(f, "Configuration error: {}", err),
            Error::Log(err) => write!(f, "Logging error: {}", err),
        }
    }
}

impl std::error::Error for Error {}

// Error conversions for `?` propagation at crate boundaries.

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<config::ConfigError> for Error {
    fn from(err: config::ConfigError) -> Self {
        Error::Config(err)
    }
}

impl From<log::LogError> for Error {
    fn from(err: log::LogError) -> Self {
        Error::Log(err)
    }
}
