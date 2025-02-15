// Imports CSH specific modules
pub mod cli;
pub mod config;
pub mod highlighter;
pub mod logging;
pub mod process;
pub mod utils;
pub mod vault;

use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    // Config(config::ConfigError),
    // Highlight(highlighter::HighlightError),
    Log(logging::LogError),
    // Vault(vault::VaultError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            // Error::Config(e) => write!(f, "Configuration error: {}", e),
            // Error::Highlight(e) => write!(f, "Highlighting error: {}", e),
            Error::Log(e) => write!(f, "Logging error: {}", e),
            // Error::Vault(e) => write!(f, "Vault error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

// Implement From for each error type
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

// impl From<config::ConfigError> for Error {
//     fn from(err: config::ConfigError) -> Self {
//         Error::Config(err)
//     }
// }

// impl From<highlighter::HighlightError> for Error {
//     fn from(err: highlighter::HighlightError) -> Self {
//         Error::Highlight(err)
//     }
// }

// impl From<logging::LogError> for Error {
//     fn from(err: logging::LogError) -> Self {
//         Error::Log(err)
//     }
// }

// impl From<vault::VaultError> for Error {
//     fn from(err: vault::VaultError) -> Self {
//         Error::Vault(err)
//     }
// }
