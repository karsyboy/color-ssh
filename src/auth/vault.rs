//! Encrypted local password vault.
//!
//! Vault data is stored under `~/.color-ssh/vault` with restrictive
//! permissions and authenticated encryption at rest.

use crate::auth::secret::{SensitiveString, sensitive_string};
use crate::validation::validate_vault_entry_name;
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use chrono::Utc;
use getrandom::fill as random_fill;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

const VAULT_VERSION: u8 = 1;
const VAULT_METADATA_FILENAME: &str = "metadata.json";
const VAULT_ENTRIES_DIRNAME: &str = "entries";
const VAULT_DIRNAME: &str = "vault";
const RUN_DIRNAME: &str = "run";
const DATA_KEY_LEN: usize = 32;
const KDF_SALT_LEN: usize = 16;
const WRAPPED_KEY_NONCE_LEN: usize = 24;
const ENTRY_NONCE_LEN: usize = 24;
const KDF_MEMORY_KIB: u32 = 64 * 1024;
const KDF_TIME_COST: u32 = 3;
const KDF_PARALLELISM: u32 = 1;
const WRAPPED_KEY_AAD: &[u8] = b"color-ssh/vault-metadata/v1";
const ENTRY_AAD_PREFIX: &[u8] = b"color-ssh/vault-entry/v1:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Vault metadata containing wrapped key material and KDF settings.
pub struct VaultMetadata {
    pub version: u8,
    pub kdf_salt: String,
    pub kdf_memory_kib: u32,
    pub kdf_time_cost: u32,
    pub kdf_parallelism: u32,
    pub wrapped_dek_nonce: String,
    pub wrapped_dek_ciphertext: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Encrypted vault entry payload.
pub struct VaultEntry {
    pub version: u8,
    pub name: String,
    pub nonce: String,
    pub ciphertext: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
/// Filesystem paths used by vault and agent runtime data.
pub struct VaultPaths {
    base_dir: PathBuf,
}

#[derive(Debug)]
/// Errors returned by vault operations.
pub enum VaultError {
    MissingHomeDirectory,
    InvalidEntryName,
    VaultAlreadyInitialized,
    VaultNotInitialized,
    EntryNotFound,
    InvalidMasterPassword,
    InvalidVaultFormat(String),
    EncryptFailed(String),
    Io(io::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHomeDirectory => write!(f, "could not determine home directory"),
            Self::InvalidEntryName => write!(f, "invalid pass entry name; use only letters, numbers, '.', '_' or '-'"),
            Self::VaultAlreadyInitialized => write!(f, "password vault is already initialized"),
            Self::VaultNotInitialized => write!(f, "password vault is not initialized"),
            Self::EntryNotFound => write!(f, "password vault entry was not found"),
            Self::InvalidMasterPassword => write!(f, "invalid master password"),
            Self::InvalidVaultFormat(message) => write!(f, "invalid vault data: {message}"),
            Self::EncryptFailed(message) => write!(f, "vault encryption failed: {message}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl VaultPaths {
    /// Resolve default paths rooted at `~/.color-ssh`.
    pub fn resolve_default() -> Result<Self, VaultError> {
        let Some(home_dir) = dirs::home_dir() else {
            return Err(VaultError::MissingHomeDirectory);
        };
        Ok(Self {
            base_dir: home_dir.join(".color-ssh"),
        })
    }

    #[cfg(test)]
    pub(crate) fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Base directory used for all `color-ssh` data files.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Path to the encrypted vault directory.
    pub fn vault_dir(&self) -> PathBuf {
        self.base_dir.join(VAULT_DIRNAME)
    }

    /// Path to vault metadata JSON.
    pub fn metadata_path(&self) -> PathBuf {
        self.vault_dir().join(VAULT_METADATA_FILENAME)
    }

    /// Directory containing encrypted entry JSON files.
    pub fn entries_dir(&self) -> PathBuf {
        self.vault_dir().join(VAULT_ENTRIES_DIRNAME)
    }

    /// Path to one entry file after name validation.
    pub fn entry_path(&self, name: &str) -> Result<PathBuf, VaultError> {
        if !validate_vault_entry_name(name) {
            return Err(VaultError::InvalidEntryName);
        }
        Ok(self.entries_dir().join(format!("{name}.json")))
    }

    /// Runtime directory used by unlock-agent IPC/event files.
    pub fn run_dir(&self) -> PathBuf {
        self.base_dir.join(RUN_DIRNAME)
    }
}

#[derive(Debug)]
/// Unlocked vault handle carrying decrypted data key material.
pub struct UnlockedVault {
    paths: VaultPaths,
    data_key: Zeroizing<[u8; DATA_KEY_LEN]>,
}

impl UnlockedVault {
    pub(crate) fn from_data_key(paths: VaultPaths, data_key: [u8; DATA_KEY_LEN]) -> Self {
        Self {
            paths,
            data_key: Zeroizing::new(data_key),
        }
    }

    /// Encrypt and store one secret under `name`.
    pub fn store_secret(&self, name: &str, secret: &str) -> Result<(), VaultError> {
        if !validate_vault_entry_name(name) {
            return Err(VaultError::InvalidEntryName);
        }

        ensure_vault_layout(&self.paths)?;

        let mut nonce = [0u8; ENTRY_NONCE_LEN];
        random_fill(&mut nonce).map_err(|err| VaultError::EncryptFailed(format!("secure random generation failed: {err}")))?;

        let cipher =
            XChaCha20Poly1305::new_from_slice(&self.data_key[..]).map_err(|err| VaultError::EncryptFailed(format!("invalid cipher key material: {err}")))?;
        let aad = entry_aad(name);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: secret.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| VaultError::EncryptFailed("failed to encrypt vault entry".to_string()))?;

        let entry = VaultEntry {
            version: VAULT_VERSION,
            name: name.to_string(),
            nonce: BASE64.encode(nonce),
            ciphertext: BASE64.encode(ciphertext),
            updated_at: Utc::now().to_rfc3339(),
        };
        write_json_atomic(&self.paths.entry_path(name)?, &entry)?;
        set_restrictive_file_permissions(&self.paths.entry_path(name)?)?;
        Ok(())
    }

    /// Decrypt and return one secret by `name`.
    pub fn get_secret(&self, name: &str) -> Result<SensitiveString, VaultError> {
        if !validate_vault_entry_name(name) {
            return Err(VaultError::InvalidEntryName);
        }

        let path = self.paths.entry_path(name)?;
        if !path.is_file() {
            return Err(VaultError::EntryNotFound);
        }

        let entry = read_json::<VaultEntry>(&path)?;
        if entry.version != VAULT_VERSION {
            return Err(VaultError::InvalidVaultFormat("unsupported entry version".to_string()));
        }
        if entry.name != name {
            return Err(VaultError::InvalidVaultFormat("entry name did not match file name".to_string()));
        }

        let nonce = decode_fixed::<ENTRY_NONCE_LEN>(&entry.nonce, "entry nonce")?;
        let ciphertext = decode_bytes(&entry.ciphertext, "entry ciphertext")?;
        if ciphertext.is_empty() {
            return Err(VaultError::InvalidVaultFormat("entry ciphertext was empty".to_string()));
        }

        let cipher =
            XChaCha20Poly1305::new_from_slice(&self.data_key[..]).map_err(|err| VaultError::EncryptFailed(format!("invalid cipher key material: {err}")))?;
        let aad = entry_aad(name);
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: ciphertext.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| VaultError::InvalidMasterPassword)?;
        match String::from_utf8(plaintext) {
            Ok(secret) => Ok(sensitive_string(secret)),
            Err(err) => {
                let mut invalid_bytes = err.into_bytes();
                invalid_bytes.zeroize();
                Err(VaultError::InvalidVaultFormat("entry plaintext was not valid UTF-8".to_string()))
            }
        }
    }

    /// Remove one secret entry by `name`.
    pub fn remove_entry(&self, name: &str) -> Result<(), VaultError> {
        let path = self.paths.entry_path(name)?;
        if !path.exists() {
            return Err(VaultError::EntryNotFound);
        }
        fs::remove_file(path)?;
        Ok(())
    }

    /// Return paths associated with this unlocked vault handle.
    pub fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub(crate) fn data_key_copy(&self) -> [u8; DATA_KEY_LEN] {
        *self.data_key
    }
}

/// Returns whether the default vault is initialized.
pub fn vault_exists() -> Result<bool, VaultError> {
    Ok(VaultPaths::resolve_default()?.metadata_path().is_file())
}

/// List all entry names in the default vault.
pub fn list_entries() -> Result<Vec<String>, VaultError> {
    list_entries_with_paths(&VaultPaths::resolve_default()?)
}

/// Returns whether the named entry exists in the default vault.
pub fn entry_exists(name: &str) -> Result<bool, VaultError> {
    entry_exists_with_paths(&VaultPaths::resolve_default()?, name)
}

/// Initialize the default vault with a master password.
pub fn initialize_vault(master_password: &str) -> Result<(), VaultError> {
    initialize_vault_with_paths(&VaultPaths::resolve_default()?, master_password)
}

/// Unlock the default vault and return a handle for entry operations.
pub fn unlock_with_password(master_password: &str) -> Result<UnlockedVault, VaultError> {
    unlock_with_password_and_paths(&VaultPaths::resolve_default()?, master_password)
}

/// Rotate the default vault master password.
pub fn rotate_master_password(current_password: &str, new_password: &str) -> Result<(), VaultError> {
    rotate_master_password_with_paths(&VaultPaths::resolve_default()?, current_password, new_password)
}

pub(crate) fn initialize_vault_with_paths(paths: &VaultPaths, master_password: &str) -> Result<(), VaultError> {
    if master_password.is_empty() {
        return Err(VaultError::InvalidMasterPassword);
    }
    if paths.metadata_path().exists() {
        return Err(VaultError::VaultAlreadyInitialized);
    }

    ensure_vault_layout(paths)?;

    let mut data_key = [0u8; DATA_KEY_LEN];
    random_fill(&mut data_key).map_err(|err| VaultError::EncryptFailed(format!("secure random generation failed: {err}")))?;
    let metadata = build_metadata_from_data_key(master_password, &data_key)?;
    data_key.zeroize();
    write_json_atomic(&paths.metadata_path(), &metadata)?;
    set_restrictive_file_permissions(&paths.metadata_path())?;
    Ok(())
}

pub(crate) fn unlock_with_password_and_paths(paths: &VaultPaths, master_password: &str) -> Result<UnlockedVault, VaultError> {
    if master_password.is_empty() {
        return Err(VaultError::InvalidMasterPassword);
    }
    let metadata_path = paths.metadata_path();
    if !metadata_path.is_file() {
        return Err(VaultError::VaultNotInitialized);
    }

    let metadata = read_json::<VaultMetadata>(&metadata_path)?;
    let data_key = decrypt_wrapped_data_key(master_password, &metadata)?;

    Ok(UnlockedVault {
        paths: paths.clone(),
        data_key: Zeroizing::new(data_key),
    })
}

pub(crate) fn rotate_master_password_with_paths(paths: &VaultPaths, current_password: &str, new_password: &str) -> Result<(), VaultError> {
    if new_password.is_empty() {
        return Err(VaultError::InvalidMasterPassword);
    }
    let unlocked = unlock_with_password_and_paths(paths, current_password)?;
    let metadata_path = paths.metadata_path();
    let existing = read_json::<VaultMetadata>(&metadata_path)?;
    let mut updated = build_metadata_from_data_key(new_password, &unlocked.data_key_copy())?;
    updated.created_at = existing.created_at;
    updated.updated_at = Utc::now().to_rfc3339();
    write_json_atomic(&metadata_path, &updated)?;
    set_restrictive_file_permissions(&metadata_path)?;
    Ok(())
}

pub(crate) fn list_entries_with_paths(paths: &VaultPaths) -> Result<Vec<String>, VaultError> {
    if !paths.metadata_path().is_file() {
        return Err(VaultError::VaultNotInitialized);
    }

    let entries_dir = paths.entries_dir();
    if !entries_dir.exists() {
        return Ok(Vec::new());
    }
    if !entries_dir.is_dir() {
        return Err(VaultError::InvalidVaultFormat("entries path was not a directory".to_string()));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(entries_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }

        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return Err(VaultError::InvalidVaultFormat("entry file name was not valid UTF-8".to_string()));
        };
        if !validate_vault_entry_name(name) {
            return Err(VaultError::InvalidVaultFormat(format!("invalid entry file name: {name}")));
        }
        entries.push(name.to_string());
    }

    entries.sort_unstable();
    Ok(entries)
}

pub(crate) fn entry_exists_with_paths(paths: &VaultPaths, name: &str) -> Result<bool, VaultError> {
    if !validate_vault_entry_name(name) {
        return Err(VaultError::InvalidEntryName);
    }
    if !paths.metadata_path().is_file() {
        return Err(VaultError::VaultNotInitialized);
    }

    Ok(paths.entry_path(name)?.is_file())
}

fn build_metadata_from_data_key(master_password: &str, data_key: &[u8; DATA_KEY_LEN]) -> Result<VaultMetadata, VaultError> {
    let mut salt = [0u8; KDF_SALT_LEN];
    random_fill(&mut salt).map_err(|err| VaultError::EncryptFailed(format!("secure random generation failed: {err}")))?;
    let mut nonce = [0u8; WRAPPED_KEY_NONCE_LEN];
    random_fill(&mut nonce).map_err(|err| VaultError::EncryptFailed(format!("secure random generation failed: {err}")))?;

    let mut wrapping_key = Zeroizing::new([0u8; DATA_KEY_LEN]);
    derive_key(master_password.as_bytes(), &salt, &mut wrapping_key)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&wrapping_key[..]).map_err(|err| VaultError::EncryptFailed(format!("invalid cipher key material: {err}")))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: data_key,
                aad: WRAPPED_KEY_AAD,
            },
        )
        .map_err(|_| VaultError::EncryptFailed("failed to wrap data key".to_string()))?;

    let now = Utc::now().to_rfc3339();
    Ok(VaultMetadata {
        version: VAULT_VERSION,
        kdf_salt: BASE64.encode(salt),
        kdf_memory_kib: KDF_MEMORY_KIB,
        kdf_time_cost: KDF_TIME_COST,
        kdf_parallelism: KDF_PARALLELISM,
        wrapped_dek_nonce: BASE64.encode(nonce),
        wrapped_dek_ciphertext: BASE64.encode(ciphertext),
        created_at: now.clone(),
        updated_at: now,
    })
}

fn decrypt_wrapped_data_key(master_password: &str, metadata: &VaultMetadata) -> Result<[u8; DATA_KEY_LEN], VaultError> {
    if metadata.version != VAULT_VERSION {
        return Err(VaultError::InvalidVaultFormat("unsupported vault version".to_string()));
    }
    if metadata.kdf_memory_kib == 0 || metadata.kdf_time_cost == 0 || metadata.kdf_parallelism == 0 {
        return Err(VaultError::InvalidVaultFormat("invalid KDF parameters".to_string()));
    }

    let salt = decode_fixed::<KDF_SALT_LEN>(&metadata.kdf_salt, "KDF salt")?;
    let nonce = decode_fixed::<WRAPPED_KEY_NONCE_LEN>(&metadata.wrapped_dek_nonce, "wrapped DEK nonce")?;
    let ciphertext = decode_bytes(&metadata.wrapped_dek_ciphertext, "wrapped DEK ciphertext")?;

    let params = Params::new(metadata.kdf_memory_kib, metadata.kdf_time_cost, metadata.kdf_parallelism, Some(DATA_KEY_LEN))
        .map_err(|err| VaultError::InvalidVaultFormat(format!("invalid KDF parameters: {err}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut wrapping_key = Zeroizing::new([0u8; DATA_KEY_LEN]);
    argon2
        .hash_password_into(master_password.as_bytes(), &salt, &mut wrapping_key[..])
        .map_err(|_| VaultError::InvalidMasterPassword)?;

    let cipher =
        XChaCha20Poly1305::new_from_slice(&wrapping_key[..]).map_err(|err| VaultError::EncryptFailed(format!("invalid cipher key material: {err}")))?;
    let mut plaintext = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: ciphertext.as_slice(),
                aad: WRAPPED_KEY_AAD,
            },
        )
        .map_err(|_| VaultError::InvalidMasterPassword)?;
    if plaintext.len() != DATA_KEY_LEN {
        plaintext.zeroize();
        return Err(VaultError::InvalidVaultFormat("wrapped DEK plaintext had the wrong length".to_string()));
    }

    let mut data_key = [0u8; DATA_KEY_LEN];
    data_key.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(data_key)
}

fn derive_key(passphrase: &[u8], salt: &[u8], key_output: &mut [u8; DATA_KEY_LEN]) -> Result<(), VaultError> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_TIME_COST, KDF_PARALLELISM, Some(DATA_KEY_LEN))
        .map_err(|err| VaultError::EncryptFailed(format!("invalid KDF parameters: {err}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password_into(passphrase, salt, key_output)
        .map_err(|err| VaultError::EncryptFailed(format!("failed to derive encryption key: {err}")))
}

fn ensure_vault_layout(paths: &VaultPaths) -> Result<(), VaultError> {
    fs::create_dir_all(paths.vault_dir())?;
    set_restrictive_directory_permissions(&paths.vault_dir())?;
    fs::create_dir_all(paths.entries_dir())?;
    set_restrictive_directory_permissions(&paths.entries_dir())?;
    fs::create_dir_all(paths.run_dir())?;
    set_restrictive_directory_permissions(&paths.run_dir())?;
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), VaultError> {
    let Some(parent) = path.parent() else {
        return Err(VaultError::Io(io::Error::other("invalid output path")));
    };
    fs::create_dir_all(parent)?;
    set_restrictive_directory_permissions(parent)?;

    let serialized = serde_json::to_vec_pretty(value).map_err(|err| VaultError::InvalidVaultFormat(format!("failed to serialize JSON: {err}")))?;
    let file_name = path.file_name().and_then(|segment| segment.to_str()).unwrap_or("vault-data");
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}", Utc::now().timestamp_nanos_opt().unwrap_or_default()));
    fs::write(&tmp_path, serialized)?;
    set_restrictive_file_permissions(&tmp_path)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, VaultError> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|err| VaultError::InvalidVaultFormat(format!("failed to parse JSON: {err}")))
}

fn decode_bytes(encoded: &str, label: &str) -> Result<Vec<u8>, VaultError> {
    BASE64
        .decode(encoded)
        .map_err(|err| VaultError::InvalidVaultFormat(format!("failed to decode {label}: {err}")))
}

fn decode_fixed<const N: usize>(encoded: &str, label: &str) -> Result<[u8; N], VaultError> {
    let decoded = decode_bytes(encoded, label)?;
    if decoded.len() != N {
        return Err(VaultError::InvalidVaultFormat(format!("{label} had the wrong length")));
    }
    let mut output = [0u8; N];
    output.copy_from_slice(&decoded);
    Ok(output)
}

fn entry_aad(name: &str) -> String {
    format!("{}{}", String::from_utf8_lossy(ENTRY_AAD_PREFIX), name)
}

fn set_restrictive_directory_permissions(path: &Path) -> Result<(), VaultError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn set_restrictive_file_permissions(path: &Path) -> Result<(), VaultError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(test)]
#[path = "../test/auth/vault.rs"]
mod tests;
