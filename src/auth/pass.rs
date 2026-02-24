use crate::log_debug;
use std::collections::HashMap;
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

#[cfg(test)]
#[path = "../test/auth/pass.rs"]
mod tests;
