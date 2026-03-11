//! Errors returned by unlock-agent operations.

use crate::auth::vault::VaultError;
use std::fmt;
use std::io;

#[derive(Debug)]
/// Agent client/server error variants.
pub enum AgentError {
    Vault(VaultError),
    Io(io::Error),
    Locked,
    EntryNotFound,
    InvalidMasterPassword,
    InvalidOrExpiredAskpassToken,
    VaultNotInitialized,
    Protocol(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Locked => write!(f, "password vault is locked"),
            Self::EntryNotFound => write!(f, "password vault entry was not found"),
            Self::InvalidMasterPassword => write!(f, "invalid master password"),
            Self::InvalidOrExpiredAskpassToken => write!(f, "invalid or expired askpass token"),
            Self::VaultNotInitialized => write!(f, "password vault is not initialized"),
            Self::Protocol(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<io::Error> for AgentError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<VaultError> for AgentError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

pub(crate) fn map_remote_error(code: &str, message: String) -> AgentError {
    match code {
        "locked" => AgentError::Locked,
        "entry_not_found" => AgentError::EntryNotFound,
        "invalid_master_password" => AgentError::InvalidMasterPassword,
        "invalid_or_expired_askpass_token" => AgentError::InvalidOrExpiredAskpassToken,
        "vault_not_initialized" => AgentError::VaultNotInitialized,
        "invalid_entry_name" | "vault_error" | "askpass_token_error" => AgentError::Protocol(message),
        _ => AgentError::Protocol(message),
    }
}

#[cfg(test)]
#[path = "../../test/auth/agent/error.rs"]
mod tests;
