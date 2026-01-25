//! color-ssh (csh) - A Rust-based SSH client wrapper with syntax highlighting
//!
//! This library provides the core functionality for the color-ssh tool, which wraps
//! SSH connections with syntax highlighting and enhanced logging capabilities.
//!
//! # Features
//!
//! - **Syntax Highlighting**: Apply ANSI color codes to SSH output based on regex patterns
//! - **Configuration Management**: YAML-based configuration with hot-reloading
//! - **Logging**: Comprehensive debug and session logging
//! - **Process Management**: Robust SSH subprocess handling with proper error propagation
//!
//! # Modules
//!
//! - [`args`] - Command-line argument parsing
//! - [`config`] - Configuration loading, watching, and management
//! - [`highlighter`] - Syntax highlighting engine with regex pattern matching
//! - [`log`] - Structured logging with multiple levels and targets
//! - [`process`] - SSH subprocess spawning and I/O handling
//!
//! # Examples
//!
//! Basic usage as a library:
//!
//! ```no_run
//! use csh::{args, config, process};
//!
//! fn main() -> csh::Result<std::process::ExitCode> {
//!     // Parse arguments
//!     let args = args::main_args();
//!     
//!     // Initialize config watcher
//!     let _watcher = config::config_watcher();
//!     
//!     // Run SSH process
//!     process::process_handler(args.ssh_args, args.is_non_interactive)
//! }
//! ```

// Imports CSH specific modules
pub mod args;
pub mod config;
pub mod highlighter;
pub mod log;
pub mod process;

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
    /// Syntax highlighting error
    Highlight(highlighter::HighlightError),
    /// Logging operation error
    Log(log::LogError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::Config(err) => write!(f, "Configuration error: {}", err),
            Error::Highlight(err) => write!(f, "Highlighting error: {}", err),
            Error::Log(err) => write!(f, "Logging error: {}", err)
        }
    }
}

impl std::error::Error for Error {}

// Implement From for each error type to enable easy error conversion

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

impl From<highlighter::HighlightError> for Error {
    fn from(err: highlighter::HighlightError) -> Self {
        Error::Highlight(err)
    }
}

impl From<log::LogError> for Error {
    fn from(err: log::LogError) -> Self {
        Error::Log(err)
    }
}
