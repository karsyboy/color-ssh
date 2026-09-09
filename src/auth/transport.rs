//! Internal askpass environment and prompt-classification helpers.

use crate::{log_debug, platform};
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};

pub const INTERNAL_ASKPASS_MODE_ENV: &str = "COSSH_INTERNAL_ASKPASS";
pub const INTERNAL_ASKPASS_TOKEN_ENV: &str = "COSSH_INTERNAL_ASKPASS_TOKEN";
const SSH_ASKPASS_ENV: &str = "SSH_ASKPASS";
const SSH_ASKPASS_REQUIRE_ENV: &str = "SSH_ASKPASS_REQUIRE";
const SSH_ASKPASS_FORCE: &str = "force";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Decision outcome for an incoming askpass prompt string.
pub enum AskpassPromptDecision {
    /// Prompt appears to be a standard password prompt.
    Allow,
    /// Prompt requires an explicit response from the controlling terminal.
    ConfirmInteractively,
    /// Prompt was missing/empty.
    DenyMissing,
    /// Prompt looked unsafe or unsupported for auto-login.
    DenyUnexpected,
}

/// Configure environment so OpenSSH invokes `cossh` as askpass helper.
pub fn configure_internal_askpass_env(command_env: &mut Vec<(String, String)>, token: &str) -> io::Result<()> {
    let cossh_path = platform::cossh_path()?;
    log_debug!("Configuring internal askpass helper");
    command_env.push((SSH_ASKPASS_ENV.to_string(), cossh_path.to_string_lossy().into_owned()));
    command_env.push((SSH_ASKPASS_REQUIRE_ENV.to_string(), SSH_ASKPASS_FORCE.to_string()));
    command_env.push((INTERNAL_ASKPASS_MODE_ENV.to_string(), "1".to_string()));
    command_env.push((INTERNAL_ASKPASS_TOKEN_ENV.to_string(), token.to_string()));
    Ok(())
}

/// Returns `true` when process was launched as internal askpass helper.
pub fn is_internal_askpass_invocation() -> bool {
    std::env::var_os(INTERNAL_ASKPASS_MODE_ENV).is_some() && std::env::var_os(INTERNAL_ASKPASS_TOKEN_ENV).is_some()
}

/// Read askpass token from environment.
pub fn internal_askpass_token() -> Option<String> {
    std::env::var(INTERNAL_ASKPASS_TOKEN_ENV).ok().filter(|token| !token.is_empty())
}

/// Read askpass prompt argument from process argv.
pub fn internal_askpass_prompt() -> Option<String> {
    std::env::args_os()
        .nth(1)
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|prompt| !prompt.trim().is_empty())
}

/// Classify whether a prompt should be served by vault auto-login.
pub fn classify_internal_askpass_prompt(prompt: Option<&str>) -> AskpassPromptDecision {
    let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) else {
        return AskpassPromptDecision::DenyMissing;
    };

    let normalized = prompt.to_ascii_lowercase();
    if normalized.contains("yes/no") || normalized.contains("please type 'yes' or 'no'") || normalized.contains("please type 'yes', 'no'") {
        return AskpassPromptDecision::ConfirmInteractively;
    }

    let deny_terms = [
        "passphrase",
        "pin",
        "otp",
        "token",
        "verification",
        "one-time",
        "yes/no",
        "are you sure",
        "challenge",
        "duo",
        "authenticator",
    ];

    if !normalized.contains("password") || deny_terms.iter().any(|term| normalized.contains(term)) {
        return AskpassPromptDecision::DenyUnexpected;
    }

    AskpassPromptDecision::Allow
}

/// Prompt for host confirmation on the controlling terminal, keeping stdout
/// reserved for the response returned to OpenSSH.
pub fn prompt_internal_askpass_confirmation(prompt: &str) -> io::Result<String> {
    let mut tty_writer = OpenOptions::new().write(true).open("/dev/tty")?;
    let tty_reader = BufReader::new(OpenOptions::new().read(true).open("/dev/tty")?);
    read_internal_askpass_confirmation(prompt, tty_reader, &mut tty_writer)
}

fn read_internal_askpass_confirmation(prompt: &str, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<String> {
    writer.write_all(prompt.as_bytes())?;
    writer.flush()?;

    let mut response = String::new();
    reader.read_line(&mut response)?;
    Ok(response.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(test)]
#[path = "../test/auth/transport.rs"]
mod tests;
