use crate::command_path;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordTransportBackend {
    InternalAskpass,
    UnsupportedPlatform,
}

pub const INTERNAL_ASKPASS_MODE_ENV: &str = "COSSH_INTERNAL_ASKPASS";
pub const INTERNAL_ASKPASS_ENTRY_ENV: &str = "COSSH_INTERNAL_ASKPASS_ENTRY";
const SSH_ASKPASS_ENV: &str = "SSH_ASKPASS";
const SSH_ASKPASS_REQUIRE_ENV: &str = "SSH_ASKPASS_REQUIRE";
const SSH_ASKPASS_FORCE: &str = "force";

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

pub fn configure_internal_askpass_env(command_env: &mut Vec<(String, String)>, entry_name: &str) -> io::Result<()> {
    let cossh_path = command_path::cossh_path()?;
    command_env.push((SSH_ASKPASS_ENV.to_string(), cossh_path.to_string_lossy().into_owned()));
    command_env.push((SSH_ASKPASS_REQUIRE_ENV.to_string(), SSH_ASKPASS_FORCE.to_string()));
    command_env.push((INTERNAL_ASKPASS_MODE_ENV.to_string(), "1".to_string()));
    command_env.push((INTERNAL_ASKPASS_ENTRY_ENV.to_string(), entry_name.to_string()));
    Ok(())
}

pub fn is_internal_askpass_invocation() -> bool {
    std::env::var_os(INTERNAL_ASKPASS_MODE_ENV).is_some() && std::env::var_os(INTERNAL_ASKPASS_ENTRY_ENV).is_some()
}

pub fn internal_askpass_entry() -> Option<String> {
    std::env::var(INTERNAL_ASKPASS_ENTRY_ENV).ok().filter(|entry| !entry.is_empty())
}

pub fn unsupported_transport_notice() -> String {
    "Password auto-login is not supported on this platform yet; continuing with the standard SSH password prompt.".to_string()
}
