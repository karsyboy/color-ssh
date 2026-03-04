//! CLI handlers for `cossh vault` and internal askpass mode.

use super::prompt::{
    prompt_entry_secret, prompt_existing_master_password, prompt_existing_master_password_with_label, prompt_new_master_password,
    prompt_new_master_password_with_label,
};
use crate::auth::secret::{ExposeSecret, SensitiveString};
use crate::{args, config, log_debug};
use chrono::{Local, TimeZone};
use std::process::ExitCode;

fn format_hms_duration(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn format_local_timeout_at(epoch_seconds: u64) -> String {
    Local
        .timestamp_opt(epoch_seconds as i64, 0)
        .single()
        .map(|datetime| datetime.format("%a %m-%d-%Y %I:%M:%S %p").to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn unlock_policy_from_config() -> super::ipc::UnlockPolicy {
    let auth_settings = config::auth_settings();
    log_debug!(
        "Using vault unlock policy from config: idle={}s absolute={}s",
        auth_settings.unlock_idle_timeout_seconds,
        auth_settings.unlock_absolute_timeout_seconds
    );
    super::ipc::UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
}

fn vault_command_name(vault_command: &args::VaultCommand) -> &'static str {
    match vault_command {
        args::VaultCommand::Init => "init",
        args::VaultCommand::AddPass(_) => "add",
        args::VaultCommand::RemovePass(_) => "remove",
        args::VaultCommand::List => "list",
        args::VaultCommand::Unlock => "unlock",
        args::VaultCommand::Lock => "lock",
        args::VaultCommand::Status => "status",
        args::VaultCommand::SetMasterPassword => "set-master-password",
    }
}

fn initialize_vault_if_needed() -> Result<Option<SensitiveString>, String> {
    if super::vault::vault_exists().map_err(|err| err.to_string())? {
        log_debug!("Password vault already initialized");
        return Ok(None);
    }

    log_debug!("Password vault not initialized; starting first-run setup");
    println!("Password vault is not initialized. Starting first-run setup.");
    let password = prompt_new_master_password()?;
    super::vault::initialize_vault(password.expose_secret()).map_err(|err| err.to_string())?;
    println!("Password vault initialized.");
    Ok(Some(password))
}

fn require_initialized_vault() -> Result<(), String> {
    match super::vault::vault_exists().map_err(|err| err.to_string())? {
        true => Ok(()),
        false => Err("Password vault is not initialized. Run `cossh vault init` first.".to_string()),
    }
}

fn resolve_master_password(initial_password: Option<SensitiveString>) -> Result<SensitiveString, String> {
    match initial_password {
        Some(password) => Ok(password),
        None => prompt_existing_master_password(),
    }
}

fn unlock_vault_for_cli(initial_password: Option<SensitiveString>) -> Result<super::vault::UnlockedVault, String> {
    let master_password = resolve_master_password(initial_password)?;
    super::vault::unlock_with_password(master_password.expose_secret()).map_err(|err| err.to_string())
}

fn command_failure(message: &str, err: impl std::fmt::Display) -> ExitCode {
    eprintln!("{message}: {err}");
    ExitCode::from(1)
}

fn run_vault_init_cli() -> ExitCode {
    log_debug!("Running `cossh vault init`");
    match initialize_vault_if_needed() {
        Ok(Some(_)) => ExitCode::SUCCESS,
        Ok(None) => {
            println!("Password vault is already initialized");
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to initialize password vault", err),
    }
}

fn run_add_pass_cli(pass_name: &str) -> ExitCode {
    log_debug!("Running `cossh vault add` for entry '{}'", pass_name);
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to initialize password vault", err),
    };

    let unlocked = match unlock_vault_for_cli(initial_password) {
        Ok(unlocked) => unlocked,
        Err(err) => return command_failure("Failed to unlock password vault", err),
    };

    let secret = match prompt_entry_secret() {
        Ok(secret) => secret,
        Err(err) => return command_failure("Failed to capture SSH password", err),
    };

    match unlocked.store_secret(pass_name, secret.expose_secret()) {
        Ok(()) => {
            log_debug!("Saved password vault entry '{}'", pass_name);
            println!("Saved password vault entry: {}", pass_name);
            println!("Set `vault_pass: {}` on the target host in ~/.color-ssh/cossh-inventory.yaml", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to save password vault entry", err),
    }
}

fn run_remove_pass_cli(pass_name: &str) -> ExitCode {
    log_debug!("Running `cossh vault remove` for entry '{}'", pass_name);
    if let Err(err) = require_initialized_vault() {
        eprintln!("{err}");
        return ExitCode::from(1);
    }

    let unlocked = match unlock_vault_for_cli(None) {
        Ok(unlocked) => unlocked,
        Err(err) => return command_failure("Failed to unlock password vault", err),
    };

    match unlocked.remove_entry(pass_name) {
        Ok(()) => {
            log_debug!("Removed password vault entry '{}'", pass_name);
            println!("Removed password vault entry: {}", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to remove password vault entry", err),
    }
}

fn run_list_passes_cli() -> ExitCode {
    log_debug!("Running `cossh vault list`");
    let entries = match super::vault::list_entries() {
        Ok(entries) => entries,
        Err(super::vault::VaultError::VaultNotInitialized) => {
            eprintln!("Password vault is not initialized. Run `cossh vault init` first.");
            return ExitCode::from(1);
        }
        Err(err) => return command_failure("Failed to list password vault entries", err),
    };

    log_debug!("Password vault contains {} entry(s)", entries.len());
    if entries.is_empty() {
        println!("No password vault entries found");
        return ExitCode::SUCCESS;
    }

    for entry in entries {
        println!("{entry}");
    }
    ExitCode::SUCCESS
}

fn run_unlock_cli() -> ExitCode {
    log_debug!("Running `cossh vault unlock`");
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to initialize password vault", err),
    };

    let master_password = match resolve_master_password(initial_password) {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to unlock password vault", err),
    };

    let client = match super::agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => return command_failure("Failed to start password vault agent", err),
    };

    match client.unlock(master_password.expose_secret(), unlock_policy_from_config()) {
        Ok(status) => {
            let expires = status.unlock_expires_in_seconds.unwrap_or_default();
            log_debug!("Password vault unlocked; session expires in {} seconds", expires);
            println!("Password vault unlocked");
            println!("Session expires in {} seconds", expires);
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to unlock password vault", err),
    }
}

fn run_lock_cli() -> ExitCode {
    log_debug!("Running `cossh vault lock`");
    match super::vault::vault_exists() {
        Ok(true) => {}
        Ok(false) => {
            log_debug!("Password vault lock requested but vault is not initialized");
            println!("Password vault is not initialized");
            return ExitCode::SUCCESS;
        }
        Err(err) => return command_failure("Failed to read password vault state", err),
    }

    let client = match super::agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => return command_failure("Failed to access password vault agent", err),
    };

    match client.lock() {
        Ok(_) => {
            log_debug!("Password vault locked and agent shutdown requested");
            println!("Password vault locked");
            ExitCode::SUCCESS
        }
        Err(super::agent::AgentError::Io(_)) => {
            log_debug!("Password vault lock requested while agent was already offline");
            println!("Password vault already locked");
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to lock password vault", err),
    }
}

fn run_vault_status_cli() -> ExitCode {
    log_debug!("Running `cossh vault status`");
    let client = match super::agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => return command_failure("Failed to access password vault agent", err),
    };

    match client.status() {
        Ok(status) => {
            log_debug!(
                "Password vault status: exists={}, unlocked={}, expires_in={:?}, absolute_timeout={:?}",
                status.vault_exists,
                status.unlocked,
                status.unlock_expires_in_seconds,
                status.absolute_timeout_seconds
            );
            println!("Vault Exist: {}", status.vault_exists);
            println!("Unlocked: {}", status.unlocked);
            if let Some(expires) = status.unlock_expires_in_seconds {
                println!("Ideal Timeout: {}", format_hms_duration(expires));
            }
            if let Some(absolute_timeout_at_epoch_seconds) = status.absolute_timeout_at_epoch_seconds {
                println!("Session Timeout: {}", format_local_timeout_at(absolute_timeout_at_epoch_seconds));
            }
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to read password vault status", err),
    }
}

fn run_set_master_password_cli() -> ExitCode {
    log_debug!("Running `cossh vault set-master-password`");
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to initialize password vault", err),
    };

    if initial_password.is_some() {
        log_debug!("Password vault initialized with a new master password");
        println!("Password vault master password set");
        return ExitCode::SUCCESS;
    }

    let current_password = match prompt_existing_master_password_with_label("current") {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to capture current master password", err),
    };
    let new_password = match prompt_new_master_password_with_label("new") {
        Ok(password) => password,
        Err(err) => return command_failure("Failed to capture new master password", err),
    };

    match super::vault::rotate_master_password(current_password.expose_secret(), new_password.expose_secret()) {
        Ok(()) => {
            let _ = run_lock_cli();
            log_debug!("Password vault master password rotated successfully");
            println!("Password vault master password updated");
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to rotate password vault master password", err),
    }
}

pub(crate) fn run_internal_askpass() -> ExitCode {
    log_debug!("Handling internal askpass invocation");
    let prompt = super::transport::internal_askpass_prompt();
    let prompt_decision = super::transport::classify_internal_askpass_prompt(prompt.as_deref());
    log_debug!("Internal askpass prompt decision: {:?}", prompt_decision);
    if prompt_decision != super::transport::AskpassPromptDecision::Allow {
        eprintln!("Password auto-login is unavailable for this SSH prompt.");
        return ExitCode::from(1);
    }

    let Some(token) = super::transport::internal_askpass_token() else {
        eprintln!("Missing internal askpass token");
        return ExitCode::from(1);
    };
    let token = SensitiveString::from_owned_string(token);

    let client = match super::agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => return command_failure("Failed to access password vault agent", err),
    };

    let secret = match client.get_secret(token.expose_secret()) {
        Ok(secret) => secret,
        Err(err) => return command_failure("Failed to read password vault entry", err),
    };

    let result = {
        use std::io::Write;

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        stdout
            .write_all(secret.expose_secret().as_bytes())
            .and_then(|_| stdout.write_all(b"\n"))
            .and_then(|_| stdout.flush())
    };

    match result {
        Ok(()) => {
            log_debug!("Internal askpass helper returned a vault secret to OpenSSH");
            ExitCode::SUCCESS
        }
        Err(err) => command_failure("Failed to write askpass response", err),
    }
}

pub(crate) fn run_vault_command(vault_command: &args::VaultCommand) -> ExitCode {
    log_debug!("Dispatching vault command '{}'", vault_command_name(vault_command));
    match vault_command {
        args::VaultCommand::Init => run_vault_init_cli(),
        args::VaultCommand::AddPass(pass_name) => run_add_pass_cli(pass_name),
        args::VaultCommand::RemovePass(pass_name) => run_remove_pass_cli(pass_name),
        args::VaultCommand::List => run_list_passes_cli(),
        args::VaultCommand::Unlock => run_unlock_cli(),
        args::VaultCommand::Lock => run_lock_cli(),
        args::VaultCommand::Status => run_vault_status_cli(),
        args::VaultCommand::SetMasterPassword => run_set_master_password_cli(),
    }
}
