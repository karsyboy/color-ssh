use crate::{command_path, log_debug};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordTransportBackend {
    InternalAskpass,
    UnsupportedPlatform,
}

pub const INTERNAL_ASKPASS_MODE_ENV: &str = "COSSH_INTERNAL_ASKPASS";
pub const INTERNAL_ASKPASS_TOKEN_ENV: &str = "COSSH_INTERNAL_ASKPASS_TOKEN";
const SSH_ASKPASS_ENV: &str = "SSH_ASKPASS";
const SSH_ASKPASS_REQUIRE_ENV: &str = "SSH_ASKPASS_REQUIRE";
const SSH_ASKPASS_FORCE: &str = "force";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskpassPromptDecision {
    Allow,
    DenyMissing,
    DenyUnexpected,
}

pub fn direct_backend() -> PasswordTransportBackend {
    #[cfg(any(unix, windows))]
    {
        PasswordTransportBackend::InternalAskpass
    }

    #[cfg(not(any(unix, windows)))]
    {
        PasswordTransportBackend::UnsupportedPlatform
    }
}

pub fn configure_internal_askpass_env(command_env: &mut Vec<(String, String)>, token: &str) -> io::Result<()> {
    let cossh_path = command_path::cossh_path()?;
    log_debug!("Configuring internal askpass helper");
    command_env.push((SSH_ASKPASS_ENV.to_string(), cossh_path.to_string_lossy().into_owned()));
    command_env.push((SSH_ASKPASS_REQUIRE_ENV.to_string(), SSH_ASKPASS_FORCE.to_string()));
    command_env.push((INTERNAL_ASKPASS_MODE_ENV.to_string(), "1".to_string()));
    command_env.push((INTERNAL_ASKPASS_TOKEN_ENV.to_string(), token.to_string()));
    Ok(())
}

pub fn is_internal_askpass_invocation() -> bool {
    std::env::var_os(INTERNAL_ASKPASS_MODE_ENV).is_some() && std::env::var_os(INTERNAL_ASKPASS_TOKEN_ENV).is_some()
}

pub fn internal_askpass_token() -> Option<String> {
    std::env::var(INTERNAL_ASKPASS_TOKEN_ENV).ok().filter(|token| !token.is_empty())
}

pub fn internal_askpass_prompt() -> Option<String> {
    std::env::args_os()
        .nth(1)
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|prompt| !prompt.trim().is_empty())
}

pub fn classify_internal_askpass_prompt(prompt: Option<&str>) -> AskpassPromptDecision {
    let Some(prompt) = prompt.map(str::trim).filter(|value| !value.is_empty()) else {
        return AskpassPromptDecision::DenyMissing;
    };

    let normalized = prompt.to_ascii_lowercase();
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

pub fn unsupported_transport_notice() -> String {
    "Password auto-login is not supported on this platform yet; continuing with the standard SSH password prompt.".to_string()
}

#[cfg(test)]
#[path = "../test/auth/transport.rs"]
mod tests;
