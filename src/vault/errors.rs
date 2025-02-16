use keepass::error::{DatabaseOpenError, DatabaseSaveError};
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum VaultError {
    IoError(io::Error),
    KeepassError(DatabaseOpenError),
    SaveError(DatabaseSaveError),
    VaultNotFound(String),
    EntryNotFound(String),
    InvalidPassword,
    EmptyPassword,
    KeyFileError(String),
    LockError(String),
    UnlockError(String),
    KeyringError(keyring::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultError::IoError(e) => write!(f, "IO error: {}", e),
            VaultError::KeepassError(e) => write!(f, "KeePass error: {}", e),
            VaultError::SaveError(e) => write!(f, "Save error: {}", e),
            VaultError::VaultNotFound(msg) => write!(f, "Vault not found: {}", msg),
            VaultError::EntryNotFound(msg) => write!(f, "Entry not found: {}", msg),
            VaultError::InvalidPassword => write!(f, "Invalid password"),
            VaultError::EmptyPassword => write!(f, "Empty password"),
            VaultError::KeyFileError(msg) => write!(f, "Key file error: {}", msg),
            VaultError::LockError(msg) => write!(f, "Lock error: {}", msg),
            VaultError::UnlockError(msg) => write!(f, "Unlock error: {}", msg),
            VaultError::KeyringError(e) => write!(f, "Keyring error: {}", e),
        }
    }
}

impl From<DatabaseOpenError> for VaultError {
    fn from(error: DatabaseOpenError) -> Self {
        VaultError::KeepassError(error)
    }
}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        VaultError::IoError(error)
    }
}

impl From<DatabaseSaveError> for VaultError {
    fn from(error: DatabaseSaveError) -> Self {
        VaultError::SaveError(error)
    }
}

impl From<keyring::Error> for VaultError {
    fn from(error: keyring::Error) -> Self {
        VaultError::KeyringError(error)
    }
}
