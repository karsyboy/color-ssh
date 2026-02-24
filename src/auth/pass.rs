use crate::log_debug;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GPG_COMMAND: &str = "gpg";
const PASS_TOOL_COMMAND: &str = "sshpass";
const MAX_DECRYPT_ATTEMPTS: usize = 3;
const GPG_PROBE_ARG: &str = "--version";
const PASS_TOOL_PROBE_ARG: &str = "-V";
const FALLBACK_NOTICE: &str = "Password auto-login unavailable; falling back to standard SSH password prompt.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassFallbackReason {
    InvalidPassKeyName,
    MissingKeyFile,
    MissingGpg,
    MissingPassTool,
    DecryptFailedAfterRetries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassResolveResult {
    Ready(String),
    Disabled,
    Fallback(PassFallbackReason),
}

#[derive(Debug)]
pub enum PassCreateError {
    InvalidPassKeyName,
    MissingHomeDirectory,
    MissingGpg,
    PasswordMismatch,
    EmptyPassword,
    OverwriteDeclined,
    GpgFailed(String),
    Io(io::Error),
}

impl fmt::Display for PassCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPassKeyName => write!(f, "invalid pass name; use only letters, numbers, '.', '_' or '-'"),
            Self::MissingHomeDirectory => write!(f, "could not determine home directory"),
            Self::MissingGpg => write!(f, "gpg is required but was not found in PATH"),
            Self::PasswordMismatch => write!(f, "password confirmation did not match"),
            Self::EmptyPassword => write!(f, "password cannot be empty"),
            Self::OverwriteDeclined => write!(f, "existing key file was not overwritten"),
            Self::GpgFailed(message) => write!(f, "gpg encryption failed: {}", message),
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
    passwords: HashMap<String, String>,
    gpg_available: Option<bool>,
    pass_tool_available: Option<bool>,
}

impl PassCache {
    fn get(&self, key: &str) -> Option<String> {
        self.passwords.get(key).cloned()
    }

    fn insert(&mut self, key: &str, password: String) {
        self.passwords.insert(key.to_string(), password);
    }

    #[cfg(test)]
    pub(crate) fn seed(&mut self, key: &str, password: &str) {
        self.insert(key, password.to_string());
    }

    fn gpg_available(&mut self) -> bool {
        if let Some(cached) = self.gpg_available {
            return cached;
        }
        let available = command_available(GPG_COMMAND, GPG_PROBE_ARG);
        self.gpg_available = Some(available);
        available
    }

    fn pass_tool_available(&mut self) -> bool {
        if let Some(cached) = self.pass_tool_available {
            return cached;
        }
        let available = command_available(PASS_TOOL_COMMAND, PASS_TOOL_PROBE_ARG);
        self.pass_tool_available = Some(available);
        available
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecryptError {
    Retryable,
    MissingGpg,
}

pub fn fallback_notice() -> &'static str {
    FALLBACK_NOTICE
}

pub fn validate_pass_key_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

pub fn pass_key_path(pass_key: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".color-ssh").join("keys").join(format!("{pass_key}.gpg")))
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
    if pass_key.is_empty() {
        return PassResolveResult::Disabled;
    }
    if !validate_pass_key_name(pass_key) {
        return PassResolveResult::Fallback(PassFallbackReason::InvalidPassKeyName);
    }
    if let Some(cached_password) = cache.get(pass_key) {
        return PassResolveResult::Ready(cached_password);
    }
    if !cache.pass_tool_available() {
        return PassResolveResult::Fallback(PassFallbackReason::MissingPassTool);
    }
    if !cache.gpg_available() {
        return PassResolveResult::Fallback(PassFallbackReason::MissingGpg);
    }
    let Some(key_path) = pass_key_path(pass_key) else {
        return PassResolveResult::Fallback(PassFallbackReason::MissingKeyFile);
    };
    if !key_path.is_file() {
        return PassResolveResult::Fallback(PassFallbackReason::MissingKeyFile);
    }

    match decrypt_with_retry(&key_path, decrypt_pass_from_file) {
        Ok(password) => {
            cache.insert(pass_key, password.clone());
            PassResolveResult::Ready(password)
        }
        Err(reason) => PassResolveResult::Fallback(reason),
    }
}

pub fn create_pass_key_interactive(pass_key: &str) -> Result<PathBuf, PassCreateError> {
    let output_path = pass_key_path(pass_key).ok_or(PassCreateError::MissingHomeDirectory)?;
    if !command_available(GPG_COMMAND, GPG_PROBE_ARG) {
        return Err(PassCreateError::MissingGpg);
    }

    create_pass_key_with_hooks(
        pass_key,
        output_path,
        prompt_overwrite_existing_key,
        prompt_for_pass_password,
        encrypt_payload_with_gpg,
    )
}

fn decrypt_pass_from_file(path: &Path) -> Result<String, DecryptError> {
    let output = Command::new(GPG_COMMAND).arg("--quiet").arg("--decrypt").arg(path).output();
    match output {
        Ok(output) => {
            if !output.status.success() {
                return Err(DecryptError::Retryable);
            }
            extract_password_from_plaintext(&output.stdout).ok_or(DecryptError::Retryable)
        }
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return Err(DecryptError::MissingGpg);
            }
            Err(DecryptError::Retryable)
        }
    }
}

fn decrypt_with_retry<F>(path: &Path, mut decrypt_once: F) -> Result<String, PassFallbackReason>
where
    F: FnMut(&Path) -> Result<String, DecryptError>,
{
    for attempt in 1..=MAX_DECRYPT_ATTEMPTS {
        match decrypt_once(path) {
            Ok(password) => return Ok(password),
            Err(DecryptError::MissingGpg) => return Err(PassFallbackReason::MissingGpg),
            Err(DecryptError::Retryable) => {
                log_debug!("GPG decrypt attempt {}/{} failed", attempt, MAX_DECRYPT_ATTEMPTS);
            }
        }
    }
    Err(PassFallbackReason::DecryptFailedAfterRetries)
}

fn command_available(command: &str, probe_arg: &str) -> bool {
    Command::new(command)
        .arg(probe_arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
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

    let password = prompt_password()?;
    encrypt_payload(&output_path, password.as_bytes())?;

    if !output_path.is_file() {
        return Err(PassCreateError::GpgFailed("no encrypted file was created".to_string()));
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

fn confirm_password_entry(password: String, confirm_password: String) -> Result<String, PassCreateError> {
    if password.is_empty() {
        return Err(PassCreateError::EmptyPassword);
    }
    if password != confirm_password {
        return Err(PassCreateError::PasswordMismatch);
    }
    Ok(password)
}

fn prompt_for_pass_password() -> Result<String, PassCreateError> {
    let password = rpassword::prompt_password("Enter SSH password to store: ")?;
    let confirm_password = rpassword::prompt_password("Confirm SSH password: ")?;
    confirm_password_entry(password, confirm_password)
}

fn encrypt_payload_with_gpg(output_path: &Path, plaintext: &[u8]) -> Result<(), PassCreateError> {
    let mut child = Command::new(GPG_COMMAND)
        .arg("--quiet")
        .arg("--symmetric")
        .arg("--output")
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                PassCreateError::MissingGpg
            } else {
                PassCreateError::Io(err)
            }
        })?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(PassCreateError::Io(io::Error::other("failed to open gpg stdin")));
    };
    stdin.write_all(plaintext)?;
    drop(stdin);

    let output = child.wait_with_output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(PassCreateError::GpgFailed("unknown gpg error".to_string()))
    } else {
        Err(PassCreateError::GpgFailed(stderr))
    }
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
