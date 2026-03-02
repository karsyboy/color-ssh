use crate::{command_path, log_debug};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use getrandom::fill as random_fill;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::{Zeroize, Zeroizing};

const PASS_TOOL_COMMAND: &str = "sshpass";
const MAX_DECRYPT_ATTEMPTS: usize = 3;
const PASS_TOOL_PROBE_ARG: &str = "-V";
const PASS_CACHE_TTL: Duration = Duration::from_secs(300);
const PASS_KEY_EXTENSION: &str = "key";
const PASS_KEY_MAGIC: &[u8; 4] = b"CSK1";
const PASS_KEY_VERSION: u8 = 1;
const PASS_KEY_SALT_LEN: usize = 16;
const PASS_KEY_NONCE_LEN: usize = 12;
const PASS_KEY_DERIVED_KEY_LEN: usize = 32;
const PASS_KEY_AAD: &[u8] = b"color-ssh/pass-key/v1";
const PASS_KEY_KDF_MEMORY_KIB: u32 = 64 * 1024;
const PASS_KEY_KDF_TIME_COST: u32 = 3;
const PASS_KEY_KDF_PARALLELISM: u32 = 1;
const DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN: usize = 8;
const DIRECT_CONNECT_CACHE_DIR: &str = "direct-connect-pass";
const DIRECT_CONNECT_CACHE_EXTENSION: &str = "cache";

#[derive(Debug, Clone)]
struct CachedPassword {
    value: Zeroizing<String>,
    inserted_at: Instant,
}

impl CachedPassword {
    fn new(password: String) -> Self {
        Self {
            value: Zeroizing::new(password),
            inserted_at: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassFallbackReason {
    InvalidPassKeyName,
    MissingKeyFile,
    MissingPassTool,
    DecryptFailedAfterRetries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassResolveResult {
    Ready(String),
    Fallback(PassFallbackReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassPromptStatus {
    Ready(String),
    PromptRequired,
    Fallback(PassFallbackReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassPromptSubmitResult {
    Ready(String),
    InvalidPassphrase,
    Fallback(PassFallbackReason),
}

#[derive(Debug)]
pub enum PassCreateError {
    InvalidPassKeyName,
    MissingHomeDirectory,
    PasswordMismatch,
    EmptyPassword,
    EncryptionPassphraseMismatch,
    EmptyEncryptionPassphrase,
    OverwriteDeclined,
    EncryptFailed(String),
    Io(io::Error),
}

impl fmt::Display for PassCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPassKeyName => write!(f, "invalid pass name; use only letters, numbers, '.', '_' or '-'"),
            Self::MissingHomeDirectory => write!(f, "could not determine home directory"),
            Self::PasswordMismatch => write!(f, "password confirmation did not match"),
            Self::EmptyPassword => write!(f, "password cannot be empty"),
            Self::EncryptionPassphraseMismatch => write!(f, "encryption passphrase confirmation did not match"),
            Self::EmptyEncryptionPassphrase => write!(f, "encryption passphrase cannot be empty"),
            Self::OverwriteDeclined => write!(f, "existing key file was not overwritten"),
            Self::EncryptFailed(message) => write!(f, "key encryption failed: {}", message),
            Self::Io(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for PassCreateError {}

impl From<io::Error> for PassCreateError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Default, Clone)]
pub struct PassCache {
    passwords: HashMap<String, CachedPassword>,
    pass_tool_available: Option<bool>,
}

impl PassCache {
    fn prune_expired(&mut self) {
        self.passwords.retain(|_, cached| cached.inserted_at.elapsed() <= PASS_CACHE_TTL);
    }

    fn get(&mut self, key: &str) -> Option<String> {
        self.prune_expired();
        self.passwords.get(key).map(|cached| cached.value.to_string())
    }

    fn insert(&mut self, key: &str, password: String) {
        self.prune_expired();
        self.passwords.insert(key.to_string(), CachedPassword::new(password));
    }

    #[cfg(test)]
    pub(crate) fn seed(&mut self, key: &str, password: &str) {
        self.insert(key, password.to_string());
    }

    fn pass_tool_available(&mut self) -> bool {
        cached_command_available(&mut self.pass_tool_available, PASS_TOOL_COMMAND, PASS_TOOL_PROBE_ARG)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecryptError {
    Retryable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecryptWithPassphraseError {
    InvalidPassphrase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PassPreflight {
    Ready(String),
    NeedsDecrypt(PathBuf),
    Fallback(PassFallbackReason),
}

pub fn fallback_notice(reason: PassFallbackReason) -> String {
    match reason {
        PassFallbackReason::InvalidPassKeyName => {
            "Password auto-login unavailable due to invalid #_pass key name; falling back to standard SSH password prompt.".to_string()
        }
        PassFallbackReason::MissingKeyFile => {
            "Password auto-login unavailable because the configured key file was not found; falling back to standard SSH password prompt.".to_string()
        }
        PassFallbackReason::MissingPassTool => {
            "Password auto-login unavailable because sshpass is not available; falling back to standard SSH password prompt.".to_string()
        }
        PassFallbackReason::DecryptFailedAfterRetries => {
            "Password auto-login unavailable; key decryption failed after multiple attempts. Falling back to standard SSH password prompt.".to_string()
        }
    }
}

pub fn validate_pass_key_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

pub fn pass_key_path(pass_key: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".color-ssh").join("keys").join(format!("{pass_key}.{PASS_KEY_EXTENSION}")))
}

pub fn extract_password_from_plaintext(plaintext: &[u8]) -> Option<String> {
    let plaintext = String::from_utf8_lossy(plaintext);
    let first_line = plaintext.split('\n').next()?;
    let trimmed = first_line.strip_suffix('\r').unwrap_or(first_line);
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

pub fn resolve_pass_key(pass_key: &str, cache: &mut PassCache) -> PassResolveResult {
    match preflight_pass_key(pass_key, cache) {
        PassPreflight::Ready(password) => PassResolveResult::Ready(password),
        PassPreflight::NeedsDecrypt(key_path) => match decrypt_with_retry(&key_path, decrypt_pass_from_file) {
            Ok(password) => {
                cache.insert(pass_key, password.clone());
                PassResolveResult::Ready(password)
            }
            Err(reason) => PassResolveResult::Fallback(reason),
        },
        PassPreflight::Fallback(reason) => PassResolveResult::Fallback(reason),
    }
}

pub fn resolve_pass_key_for_direct_connect(pass_key: &str, cache: &mut PassCache, persistent_cache_ttl: Duration) -> PassResolveResult {
    if persistent_cache_ttl.is_zero() {
        clear_direct_connect_cached_password(pass_key);
        return resolve_pass_key(pass_key, cache);
    }

    if let Some(password) = read_direct_connect_cached_password(pass_key) {
        cache.insert(pass_key, password.clone());
        return PassResolveResult::Ready(password);
    }

    let resolved = resolve_pass_key(pass_key, cache);
    if let PassResolveResult::Ready(password) = &resolved
        && let Err(err) = write_direct_connect_cached_password(pass_key, password, persistent_cache_ttl)
    {
        log_debug!("Failed to persist direct-connect pass cache for key {}: {}", pass_key, err);
    }
    resolved
}

pub fn resolve_pass_key_for_tui(pass_key: &str, cache: &mut PassCache) -> PassPromptStatus {
    match preflight_pass_key(pass_key, cache) {
        PassPreflight::Ready(password) => PassPromptStatus::Ready(password),
        PassPreflight::NeedsDecrypt(_) => PassPromptStatus::PromptRequired,
        PassPreflight::Fallback(reason) => PassPromptStatus::Fallback(reason),
    }
}

pub fn submit_tui_passphrase(pass_key: &str, passphrase: &str, cache: &mut PassCache) -> PassPromptSubmitResult {
    match preflight_pass_key(pass_key, cache) {
        PassPreflight::Ready(password) => PassPromptSubmitResult::Ready(password),
        PassPreflight::Fallback(reason) => PassPromptSubmitResult::Fallback(reason),
        PassPreflight::NeedsDecrypt(key_path) => match decrypt_pass_from_file_with_passphrase(&key_path, passphrase) {
            Ok(password) => {
                cache.insert(pass_key, password.clone());
                PassPromptSubmitResult::Ready(password)
            }
            Err(DecryptWithPassphraseError::InvalidPassphrase) => PassPromptSubmitResult::InvalidPassphrase,
        },
    }
}

pub fn create_pass_key_interactive(pass_key: &str) -> Result<PathBuf, PassCreateError> {
    let output_path = pass_key_path(pass_key).ok_or(PassCreateError::MissingHomeDirectory)?;

    create_pass_key_with_hooks(
        pass_key,
        output_path,
        prompt_overwrite_existing_key,
        prompt_for_pass_password,
        encrypt_payload_with_internal_cipher,
    )
}

fn decrypt_pass_from_file(path: &Path) -> Result<String, DecryptError> {
    let mut passphrase = prompt_for_decrypt_passphrase().map_err(|_| DecryptError::Retryable)?;
    let result = decrypt_pass_from_file_with_passphrase(path, &passphrase).map_err(|_| DecryptError::Retryable);
    passphrase.zeroize();
    result
}

fn decrypt_pass_from_file_with_passphrase(path: &Path, passphrase: &str) -> Result<String, DecryptWithPassphraseError> {
    let mut encrypted_payload = fs::read(path).map_err(|_| DecryptWithPassphraseError::InvalidPassphrase)?;
    let mut decrypted_payload = decrypt_payload_with_passphrase(&encrypted_payload, passphrase)?;
    encrypted_payload.zeroize();
    let extracted = extract_password_from_plaintext(&decrypted_payload).ok_or(DecryptWithPassphraseError::InvalidPassphrase);
    decrypted_payload.zeroize();
    extracted
}

fn preflight_pass_key(pass_key: &str, cache: &mut PassCache) -> PassPreflight {
    if !validate_pass_key_name(pass_key) {
        return PassPreflight::Fallback(PassFallbackReason::InvalidPassKeyName);
    }
    if let Some(cached_password) = cache.get(pass_key) {
        return PassPreflight::Ready(cached_password);
    }
    if !cache.pass_tool_available() {
        return PassPreflight::Fallback(PassFallbackReason::MissingPassTool);
    }
    let Some(key_path) = pass_key_path(pass_key) else {
        return PassPreflight::Fallback(PassFallbackReason::MissingKeyFile);
    };
    if !key_path.is_file() {
        return PassPreflight::Fallback(PassFallbackReason::MissingKeyFile);
    }
    PassPreflight::NeedsDecrypt(key_path)
}

fn decrypt_with_retry<F>(path: &Path, mut decrypt_once: F) -> Result<String, PassFallbackReason>
where
    F: FnMut(&Path) -> Result<String, DecryptError>,
{
    for attempt in 1..=MAX_DECRYPT_ATTEMPTS {
        match decrypt_once(path) {
            Ok(password) => return Ok(password),
            Err(DecryptError::Retryable) => {
                log_debug!("Pass key decrypt attempt {}/{} failed", attempt, MAX_DECRYPT_ATTEMPTS);
            }
        }
    }
    Err(PassFallbackReason::DecryptFailedAfterRetries)
}

fn direct_connect_cache_path(pass_key: &str) -> Option<PathBuf> {
    if !validate_pass_key_name(pass_key) {
        return None;
    }
    dirs::home_dir().map(|home| direct_connect_cache_path_for_home(&home, pass_key))
}

fn direct_connect_cache_path_for_home(home: &Path, pass_key: &str) -> PathBuf {
    home.join(".color-ssh")
        .join("cache")
        .join(DIRECT_CONNECT_CACHE_DIR)
        .join(format!("{pass_key}.{DIRECT_CONNECT_CACHE_EXTENSION}"))
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0)
}

fn unix_timestamp_nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or(0)
}

fn encode_direct_connect_cache_record(password: &str, expires_at_unix_secs: u64) -> Vec<u8> {
    let mut record = Vec::with_capacity(DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN + password.len());
    record.extend_from_slice(&expires_at_unix_secs.to_le_bytes());
    record.extend_from_slice(password.as_bytes());
    record
}

fn decode_direct_connect_cache_record(record: &[u8], now_unix_secs: u64) -> Option<String> {
    if record.len() < DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN {
        return None;
    }

    let mut expires_at = [0u8; DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN];
    expires_at.copy_from_slice(&record[..DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN]);
    let expires_at = u64::from_le_bytes(expires_at);
    if now_unix_secs >= expires_at {
        return None;
    }

    let password_bytes = &record[DIRECT_CONNECT_CACHE_RECORD_HEADER_LEN..];
    if password_bytes.is_empty() {
        return None;
    }

    std::str::from_utf8(password_bytes)
        .ok()
        .filter(|password| !password.is_empty())
        .map(ToString::to_string)
}

fn read_direct_connect_cached_password(pass_key: &str) -> Option<String> {
    let cache_path = direct_connect_cache_path(pass_key)?;
    read_direct_connect_cached_password_from_path(&cache_path, unix_timestamp_secs())
}

fn read_direct_connect_cached_password_from_path(cache_path: &Path, now_unix_secs: u64) -> Option<String> {
    let mut record = fs::read(cache_path).ok()?;
    let decoded = decode_direct_connect_cache_record(&record, now_unix_secs);
    record.zeroize();

    if decoded.is_none() {
        let _ = fs::remove_file(cache_path);
    }

    decoded
}

fn write_direct_connect_cached_password(pass_key: &str, password: &str, ttl: Duration) -> io::Result<()> {
    let Some(cache_path) = direct_connect_cache_path(pass_key) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid pass key name"));
    };
    let Some(parent) = cache_path.parent() else {
        return Err(io::Error::other("invalid direct-connect cache path"));
    };

    fs::create_dir_all(parent)?;
    set_restrictive_directory_permissions(parent).map_err(pass_create_error_to_io)?;

    let expires_at = unix_timestamp_secs().saturating_add(ttl.as_secs().max(1));
    let mut record = encode_direct_connect_cache_record(password, expires_at);
    let temp_path = cache_path.with_extension(format!(
        "{DIRECT_CONNECT_CACHE_EXTENSION}.{}.{}.tmp",
        std::process::id(),
        unix_timestamp_nanos()
    ));

    let write_result = (|| -> io::Result<()> {
        fs::write(&temp_path, &record)?;
        set_restrictive_file_permissions(&temp_path).map_err(pass_create_error_to_io)?;
        fs::rename(&temp_path, &cache_path)?;
        set_restrictive_file_permissions(&cache_path).map_err(pass_create_error_to_io)?;
        Ok(())
    })();

    record.zeroize();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn clear_direct_connect_cached_password(pass_key: &str) {
    if let Some(cache_path) = direct_connect_cache_path(pass_key) {
        let _ = fs::remove_file(cache_path);
    }
}

fn pass_create_error_to_io(err: PassCreateError) -> io::Error {
    match err {
        PassCreateError::Io(io_err) => io_err,
        other => io::Error::other(other.to_string()),
    }
}

fn command_available(command: &str, probe_arg: &str) -> bool {
    let Ok(command_path) = command_path::resolve_known_command_path(command) else {
        return false;
    };

    Command::new(command_path)
        .arg(probe_arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn cached_command_available(cache: &mut Option<bool>, command: &str, probe_arg: &str) -> bool {
    if let Some(cached) = *cache {
        return cached;
    }
    let available = command_available(command, probe_arg);
    *cache = Some(available);
    available
}

fn create_pass_key_with_hooks<FOverwrite, FPassword, FEncrypt>(
    pass_key: &str,
    output_path: PathBuf,
    mut prompt_overwrite: FOverwrite,
    mut prompt_password: FPassword,
    mut encrypt_payload: FEncrypt,
) -> Result<PathBuf, PassCreateError>
where
    FOverwrite: FnMut(&Path) -> Result<bool, PassCreateError>,
    FPassword: FnMut() -> Result<String, PassCreateError>,
    FEncrypt: FnMut(&Path, &[u8]) -> Result<(), PassCreateError>,
{
    if !validate_pass_key_name(pass_key) {
        return Err(PassCreateError::InvalidPassKeyName);
    }

    ensure_keys_directory_for_path(&output_path)?;

    if output_path.exists() {
        if !prompt_overwrite(&output_path)? {
            return Err(PassCreateError::OverwriteDeclined);
        }
        fs::remove_file(&output_path)?;
    }

    let mut password = prompt_password()?;
    let encrypt_result = encrypt_payload(&output_path, password.as_bytes());
    password.zeroize();
    encrypt_result?;

    if !output_path.is_file() {
        return Err(PassCreateError::EncryptFailed("no encrypted file was created".to_string()));
    }

    set_restrictive_file_permissions(&output_path)?;
    Ok(output_path)
}

fn ensure_keys_directory_for_path(path: &Path) -> Result<(), PassCreateError> {
    let Some(parent) = path.parent() else {
        return Err(PassCreateError::Io(io::Error::other("invalid output path")));
    };
    fs::create_dir_all(parent)?;
    set_restrictive_directory_permissions(parent)?;
    Ok(())
}

fn parse_overwrite_confirmation(input: &str) -> bool {
    let value = input.trim().to_ascii_lowercase();
    matches!(value.as_str(), "y" | "yes")
}

fn prompt_overwrite_existing_key(path: &Path) -> Result<bool, PassCreateError> {
    print!("Key file {} already exists. Overwrite existing key file? [y/N]: ", path.display());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(parse_overwrite_confirmation(&input))
}

fn confirm_password_entry(mut password: String, mut confirm_password: String) -> Result<String, PassCreateError> {
    if password.is_empty() {
        password.zeroize();
        confirm_password.zeroize();
        return Err(PassCreateError::EmptyPassword);
    }
    if password != confirm_password {
        password.zeroize();
        confirm_password.zeroize();
        return Err(PassCreateError::PasswordMismatch);
    }
    confirm_password.zeroize();
    Ok(password)
}

fn prompt_for_pass_password() -> Result<String, PassCreateError> {
    let password = rpassword::prompt_password("Enter SSH password to store: ")?;
    let confirm_password = rpassword::prompt_password("Confirm SSH password: ")?;
    confirm_password_entry(password, confirm_password)
}

fn encrypt_payload_with_internal_cipher(output_path: &Path, plaintext: &[u8]) -> Result<(), PassCreateError> {
    let mut encryption_passphrase = prompt_for_encrypt_passphrase()?;
    let mut encrypted_payload = encrypt_payload_for_storage(plaintext, &encryption_passphrase)?;
    let write_result = fs::write(output_path, &encrypted_payload).map_err(PassCreateError::Io);
    encrypted_payload.zeroize();
    encryption_passphrase.zeroize();
    write_result
}

fn prompt_for_encrypt_passphrase() -> Result<String, PassCreateError> {
    let mut passphrase = rpassword::prompt_password("Enter key encryption passphrase: ")?;
    let mut confirm = rpassword::prompt_password("Confirm key encryption passphrase: ")?;
    if passphrase.is_empty() {
        passphrase.zeroize();
        confirm.zeroize();
        return Err(PassCreateError::EmptyEncryptionPassphrase);
    }
    if passphrase != confirm {
        passphrase.zeroize();
        confirm.zeroize();
        return Err(PassCreateError::EncryptionPassphraseMismatch);
    }
    confirm.zeroize();
    Ok(passphrase)
}

fn prompt_for_decrypt_passphrase() -> io::Result<String> {
    rpassword::prompt_password("Enter key passphrase: ")
}

fn encrypt_payload_for_storage(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, PassCreateError> {
    let mut salt = [0u8; PASS_KEY_SALT_LEN];
    random_fill(&mut salt).map_err(|err| PassCreateError::EncryptFailed(format!("secure random generation failed: {err}")))?;
    let mut nonce = [0u8; PASS_KEY_NONCE_LEN];
    random_fill(&mut nonce).map_err(|err| PassCreateError::EncryptFailed(format!("secure random generation failed: {err}")))?;

    let mut key = Zeroizing::new([0u8; PASS_KEY_DERIVED_KEY_LEN]);
    derive_encryption_key(passphrase.as_bytes(), &salt, &mut key)?;
    let cipher = ChaCha20Poly1305::new_from_slice(&key[..]).map_err(|err| PassCreateError::EncryptFailed(format!("invalid cipher key material: {err}")))?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: PASS_KEY_AAD,
            },
        )
        .map_err(|_| PassCreateError::EncryptFailed("failed to encrypt payload".to_string()))?;

    let mut output = Vec::with_capacity(PASS_KEY_MAGIC.len() + 1 + PASS_KEY_SALT_LEN + PASS_KEY_NONCE_LEN + ciphertext.len());
    output.extend_from_slice(PASS_KEY_MAGIC);
    output.push(PASS_KEY_VERSION);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);

    salt.zeroize();
    nonce.zeroize();
    Ok(output)
}

fn decrypt_payload_with_passphrase(payload: &[u8], passphrase: &str) -> Result<Vec<u8>, DecryptWithPassphraseError> {
    let fixed_prefix_len = PASS_KEY_MAGIC.len() + 1 + PASS_KEY_SALT_LEN + PASS_KEY_NONCE_LEN;
    if payload.len() < fixed_prefix_len {
        return Err(DecryptWithPassphraseError::InvalidPassphrase);
    }

    if &payload[..PASS_KEY_MAGIC.len()] != PASS_KEY_MAGIC {
        return Err(DecryptWithPassphraseError::InvalidPassphrase);
    }

    let version = payload[PASS_KEY_MAGIC.len()];
    if version != PASS_KEY_VERSION {
        return Err(DecryptWithPassphraseError::InvalidPassphrase);
    }

    let salt_start = PASS_KEY_MAGIC.len() + 1;
    let salt_end = salt_start + PASS_KEY_SALT_LEN;
    let nonce_end = salt_end + PASS_KEY_NONCE_LEN;
    let salt = &payload[salt_start..salt_end];
    let nonce = &payload[salt_end..nonce_end];
    let ciphertext = &payload[nonce_end..];

    if ciphertext.is_empty() {
        return Err(DecryptWithPassphraseError::InvalidPassphrase);
    }

    let mut key = Zeroizing::new([0u8; PASS_KEY_DERIVED_KEY_LEN]);
    derive_encryption_key(passphrase.as_bytes(), salt, &mut key).map_err(|_| DecryptWithPassphraseError::InvalidPassphrase)?;
    let cipher = ChaCha20Poly1305::new_from_slice(&key[..]).map_err(|_| DecryptWithPassphraseError::InvalidPassphrase)?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: PASS_KEY_AAD,
            },
        )
        .map_err(|_| DecryptWithPassphraseError::InvalidPassphrase)
}

fn derive_encryption_key(passphrase: &[u8], salt: &[u8], key_output: &mut [u8; PASS_KEY_DERIVED_KEY_LEN]) -> Result<(), PassCreateError> {
    let params = Params::new(
        PASS_KEY_KDF_MEMORY_KIB,
        PASS_KEY_KDF_TIME_COST,
        PASS_KEY_KDF_PARALLELISM,
        Some(PASS_KEY_DERIVED_KEY_LEN),
    )
    .map_err(|err| PassCreateError::EncryptFailed(format!("invalid key-derivation parameters: {err}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password_into(passphrase, salt, key_output)
        .map_err(|err| PassCreateError::EncryptFailed(format!("failed to derive encryption key: {err}")))
}

#[cfg(unix)]
fn set_restrictive_directory_permissions(path: &Path) -> Result<(), PassCreateError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_directory_permissions(_path: &Path) -> Result<(), PassCreateError> {
    Ok(())
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> Result<(), PassCreateError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> Result<(), PassCreateError> {
    Ok(())
}

#[cfg(test)]
#[path = "../test/auth/pass.rs"]
mod tests;
