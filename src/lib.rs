//! `color-ssh` library crate.
//!
//! This crate exposes the runtime entrypoint and shared modules used by the
//! `cossh` binary.

pub mod args;
pub mod auth;
pub mod config;
pub mod inventory;
pub mod log;
mod platform;
mod process;
pub mod runtime;
pub mod ssh_config;
pub(crate) mod terminal;
pub mod tui;

#[cfg(test)]
mod test;

use std::io;

pub use runtime::run;

/// Result alias for crate operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type for public crate APIs.
#[derive(Debug)]
pub enum Error {
    /// Filesystem or process I/O error.
    Io(io::Error),
    /// Configuration load/parse error.
    Config(config::ConfigError),
    /// Logging initialization or write error.
    Log(log::LogError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::Config(err) => write!(f, "Configuration error: {}", err),
            Error::Log(err) => write!(f, "Logging error: {}", err),
        }
    }
}

impl std::error::Error for Error {}

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
