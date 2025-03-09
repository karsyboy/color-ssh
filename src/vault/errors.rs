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
    VaultAlreadyExists,
    KeyFileCreationFailed,
    NoVaultFile,
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultError::IoError(err) => write!(f, "IO error: {}", err),
            VaultError::KeepassError(err) => write!(f, "KeePass error: {}", err),
            VaultError::SaveError(err) => write!(f, "Save error: {}", err),
            VaultError::VaultNotFound(msg) => write!(f, "Vault not found: {}", msg),
            VaultError::EntryNotFound(msg) => write!(f, "Entry not found: {}", msg),
            VaultError::InvalidPassword => write!(f, "Invalid password"),
            VaultError::EmptyPassword => write!(f, "Empty password"),
            VaultError::KeyFileError(msg) => write!(f, "Key file error: {}", msg),
            VaultError::LockError(msg) => write!(f, "Lock error: {}", msg),
            VaultError::UnlockError(msg) => write!(f, "Unlock error: {}", msg),
            VaultError::KeyringError(err) => write!(f, "Keyring error: {}", err),
            VaultError::VaultAlreadyExists => write!(f, "Vault already exists"),
            VaultError::KeyFileCreationFailed => write!(f, "Key file creation failed"),
            VaultError::NoVaultFile => write!(f, "No vault file"),
        }
    }
}

impl From<DatabaseOpenError> for VaultError {
    fn from(err: DatabaseOpenError) -> Self {
        VaultError::KeepassError(err)
    }
}

impl From<io::Error> for VaultError {
    fn from(err: io::Error) -> Self {
        VaultError::IoError(err)
    }
}

impl From<DatabaseSaveError> for VaultError {
    fn from(err: DatabaseSaveError) -> Self {
        VaultError::SaveError(err)
    }
}

impl From<keyring::Error> for VaultError {
    fn from(err: keyring::Error) -> Self {
        VaultError::KeyringError(err)
    }
}
