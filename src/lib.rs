// Imports CSH specific modules
pub mod args;
pub mod config;
pub mod highlighter;
pub mod log;
pub mod process;
pub mod ui;
pub mod vault;

use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Config(config::ConfigError),
    Highlight(highlighter::HighlightError),
    Log(log::LogError),
    UI(ui::UIError),
    Vault(vault::VaultError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::Config(err) => write!(f, "Configuration error: {}", err),
            Error::Highlight(err) => write!(f, "Highlighting error: {}", err),
            Error::Log(err) => write!(f, "Logging error: {}", err),
            Error::UI(err) => write!(f, "UI error: {}", err),
            Error::Vault(err) => write!(f, "Vault error: {}", err),
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

impl From<ui::UIError> for Error {
    fn from(err: ui::UIError) -> Self {
        Error::UI(err)
    }
}

impl From<vault::VaultError> for Error {
    fn from(err: vault::VaultError) -> Self {
        Error::Vault(err)
    }
}
