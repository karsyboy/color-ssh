//! Shared vault-access helpers for protocol launch paths.

use crate::auth::{
    self, agent,
    ipc::UnlockPolicy,
    secret::{ExposeSecret, SensitiveString},
};
use crate::config;
use crate::log_debug;
use crate::validation::validate_vault_entry_name;
use std::fmt;
use std::io::{self, IsTerminal};

#[derive(Debug)]
pub(super) enum VaultAccessError {
    VaultNotInitialized,
    Query(String),
    LockedWithoutTerminal,
    UnlockFailed(String),
}

impl fmt::Display for VaultAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VaultNotInitialized => write!(f, "password vault is not initialized"),
            Self::Query(message) | Self::UnlockFailed(message) => f.write_str(message),
            Self::LockedWithoutTerminal => write!(f, "password vault is locked; run `cossh vault unlock`"),
        }
    }
}

fn current_unlock_policy() -> UnlockPolicy {
    let auth_settings = config::auth_settings();
    UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
}

fn unlock_agent_interactively(client: &agent::AgentClient) -> io::Result<()> {
    let policy = current_unlock_policy();
    for attempt in 1..=3 {
        log_debug!("Prompting for password vault unlock (attempt {} of 3)", attempt);
        let master_password = auth::prompt_hidden_secret("Enter vault master password: ")?;
        if master_password.expose_secret().is_empty() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "master password cannot be empty"));
        }

        // Keep retry loop local here so SSH/RDP builders can stay non-interactive.
        match client.unlock(master_password.expose_secret(), policy.clone()) {
            Ok(_) => {
                log_debug!("Interactive password vault unlock succeeded");
                return Ok(());
            }
            Err(agent::AgentError::InvalidMasterPassword) => {
                log_debug!("Interactive password vault unlock failed due to invalid master password");
                if attempt == 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "failed to unlock password vault after 3 attempts",
                    ));
                }
                eprintln!("Invalid master password. Try again.");
            }
            Err(agent::AgentError::VaultNotInitialized) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`",
                ));
            }
            Err(err) => {
                log_debug!("Interactive password vault unlock failed: {}", err);
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "failed to unlock password vault after 3 attempts",
    ))
}

pub(super) fn query_vault_entry_status(client: &agent::AgentClient, pass_entry_name: &str) -> std::result::Result<agent::AgentEntryStatus, VaultAccessError> {
    let entry_status = match client.entry_status(pass_entry_name) {
        Ok(status) => status,
        Err(agent::AgentError::VaultNotInitialized) => return Err(VaultAccessError::VaultNotInitialized),
        Err(err) => return Err(VaultAccessError::Query(err.to_string())),
    };

    if !entry_status.status.unlocked {
        log_debug!("Password vault was locked during launch preparation");
        // Non-interactive callers cannot prompt for master password.
        if !io::stdin().is_terminal() {
            return Err(VaultAccessError::LockedWithoutTerminal);
        }
        unlock_agent_interactively(client).map_err(|err| VaultAccessError::UnlockFailed(err.to_string()))?;
        log_debug!("Retrying password vault entry lookup after unlock");
        let entry_status = client
            .entry_status(pass_entry_name)
            .map_err(|err| VaultAccessError::UnlockFailed(err.to_string()))?;
        if !entry_status.status.unlocked {
            return Err(VaultAccessError::UnlockFailed("password vault remains locked after unlock attempt".to_string()));
        }
        return Ok(entry_status);
    }

    Ok(entry_status)
}

pub(super) fn resolve_vault_password(pass_entry_name: &str) -> io::Result<SensitiveString> {
    if !validate_vault_entry_name(pass_entry_name) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid password vault entry name for RDP launch"));
    }

    let client = agent::AgentClient::new().map_err(|err| io::Error::other(err.to_string()))?;
    let entry_status = match query_vault_entry_status(&client, pass_entry_name) {
        Ok(status) => status,
        Err(VaultAccessError::VaultNotInitialized) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`",
            ));
        }
        Err(VaultAccessError::LockedWithoutTerminal) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault is locked; run `cossh vault unlock`",
            ));
        }
        Err(err) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, err.to_string())),
    };

    if !entry_status.exists {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("password vault entry '{pass_entry_name}' was not found"),
        ));
    }

    // Reuse short-lived askpass token flow to avoid exposing entry names broadly.
    let askpass_token = client
        .authorize_askpass(pass_entry_name)
        .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;

    client
        .get_secret(askpass_token.expose_secret())
        .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))
}
