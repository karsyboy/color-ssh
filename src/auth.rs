pub mod agent;
pub mod ipc;
pub mod secret;
pub mod transport;
pub mod vault;

use crate::auth::secret::ExposeSecret;
use crate::{args, config, log_debug};
use std::process::ExitCode;
use zeroize::Zeroize;

fn confirm_hidden_value(prompt: &str, confirm_prompt: &str, empty_message: &str, mismatch_message: &str) -> std::result::Result<String, String> {
    let mut value = rpassword::prompt_password(prompt).map_err(|err| err.to_string())?;
    let mut confirm = rpassword::prompt_password(confirm_prompt).map_err(|err| err.to_string())?;
    if value.is_empty() {
        value.zeroize();
        confirm.zeroize();
        return Err(empty_message.to_string());
    }
    if value != confirm {
        value.zeroize();
        confirm.zeroize();
        return Err(mismatch_message.to_string());
    }
    confirm.zeroize();
    Ok(value)
}

fn prompt_new_master_password() -> std::result::Result<String, String> {
    confirm_hidden_value(
        "Enter vault master password: ",
        "Confirm vault master password: ",
        "master password cannot be empty",
        "master password confirmation did not match",
    )
}

fn prompt_new_master_password_with_label(label: &str) -> std::result::Result<String, String> {
    confirm_hidden_value(
        &format!("Enter {label} vault master password: "),
        &format!("Confirm {label} vault master password: "),
        "master password cannot be empty",
        "master password confirmation did not match",
    )
}

fn prompt_existing_master_password() -> std::result::Result<String, String> {
    let password = rpassword::prompt_password("Enter vault master password: ").map_err(|err| err.to_string())?;
    if password.is_empty() {
        return Err("master password cannot be empty".to_string());
    }
    Ok(password)
}

fn prompt_existing_master_password_with_label(label: &str) -> std::result::Result<String, String> {
    let password = rpassword::prompt_password(format!("Enter {label} vault master password: ")).map_err(|err| err.to_string())?;
    if password.is_empty() {
        return Err("master password cannot be empty".to_string());
    }
    Ok(password)
}

fn prompt_entry_secret() -> std::result::Result<String, String> {
    confirm_hidden_value(
        "Enter SSH password to store: ",
        "Confirm SSH password: ",
        "password cannot be empty",
        "password confirmation did not match",
    )
}

fn unlock_policy_from_config() -> ipc::UnlockPolicy {
    let auth_settings = config::auth_settings();
    ipc::UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
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

fn initialize_vault_if_needed() -> std::result::Result<Option<String>, String> {
    if vault::vault_exists().map_err(|err| err.to_string())? {
        log_debug!("Password vault already initialized");
        return Ok(None);
    }

    log_debug!("Password vault not initialized; starting first-run setup");
    println!("Password vault is not initialized. Starting first-run setup.");
    let password = prompt_new_master_password()?;
    if let Err(err) = vault::initialize_vault(&password) {
        let mut password = password;
        password.zeroize();
        return Err(err.to_string());
    }
    println!("Password vault initialized.");
    Ok(Some(password))
}

fn require_initialized_vault() -> std::result::Result<(), String> {
    match vault::vault_exists().map_err(|err| err.to_string())? {
        true => Ok(()),
        false => Err("Password vault is not initialized. Run `cossh vault init` first.".to_string()),
    }
}

fn resolve_master_password(initial_password: Option<String>) -> std::result::Result<String, String> {
    match initial_password {
        Some(password) => Ok(password),
        None => prompt_existing_master_password(),
    }
}

fn unlock_vault_for_cli(initial_password: Option<String>) -> std::result::Result<vault::UnlockedVault, String> {
    let mut master_password = resolve_master_password(initial_password)?;
    let result = vault::unlock_with_password(&master_password).map_err(|err| err.to_string());
    master_password.zeroize();
    result
}

fn run_vault_init_cli() -> ExitCode {
    log_debug!("Running `cossh vault init`");
    match initialize_vault_if_needed() {
        Ok(Some(mut password)) => {
            password.zeroize();
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!("Password vault is already initialized");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_add_pass_cli(pass_name: &str) -> ExitCode {
    log_debug!("Running `cossh vault add` for entry '{}'", pass_name);
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let unlocked = match unlock_vault_for_cli(initial_password) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let mut secret = match prompt_entry_secret() {
        Ok(secret) => secret,
        Err(err) => {
            eprintln!("Failed to capture SSH password: {err}");
            return ExitCode::from(1);
        }
    };

    let result = unlocked.store_secret(pass_name, &secret);
    secret.zeroize();

    match result {
        Ok(()) => {
            log_debug!("Saved password vault entry '{}'", pass_name);
            println!("Saved password vault entry: {}", pass_name);
            println!("Use in ~/.ssh/config: #_pass {}", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to save password vault entry: {err}");
            ExitCode::from(1)
        }
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
        Err(err) => {
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };

    match unlocked.remove_entry(pass_name) {
        Ok(()) => {
            log_debug!("Removed password vault entry '{}'", pass_name);
            println!("Removed password vault entry: {}", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to remove password vault entry: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_list_passes_cli() -> ExitCode {
    log_debug!("Running `cossh vault list`");
    let entries = match vault::list_entries() {
        Ok(entries) => entries,
        Err(vault::VaultError::VaultNotInitialized) => {
            eprintln!("Password vault is not initialized. Run `cossh vault init` first.");
            return ExitCode::from(1);
        }
        Err(err) => {
            eprintln!("Failed to list password vault entries: {err}");
            return ExitCode::from(1);
        }
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
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let mut master_password = match resolve_master_password(initial_password) {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let client = match agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => {
            master_password.zeroize();
            eprintln!("Failed to start password vault agent: {err}");
            return ExitCode::from(1);
        }
    };

    let result = client.unlock(&master_password, unlock_policy_from_config());
    master_password.zeroize();

    match result {
        Ok(status) => {
            let expires = status.unlock_expires_in_seconds.unwrap_or_default();
            log_debug!("Password vault unlocked; session expires in {} seconds", expires);
            println!("Password vault unlocked");
            println!("Session expires in {} seconds", expires);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to unlock password vault: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_lock_cli() -> ExitCode {
    log_debug!("Running `cossh vault lock`");
    match vault::vault_exists() {
        Ok(true) => {}
        Ok(false) => {
            log_debug!("Password vault lock requested but vault is not initialized");
            println!("Password vault is not initialized");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("Failed to read password vault state: {err}");
            return ExitCode::from(1);
        }
    }

    let client = match agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to access password vault agent: {err}");
            return ExitCode::from(1);
        }
    };

    match client.lock() {
        Ok(_) => {
            log_debug!("Password vault locked and agent shutdown requested");
            println!("Password vault locked");
            ExitCode::SUCCESS
        }
        Err(agent::AgentError::Io(_)) => {
            log_debug!("Password vault lock requested while agent was already offline");
            println!("Password vault already locked");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to lock password vault: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_vault_status_cli() -> ExitCode {
    log_debug!("Running `cossh vault status`");
    let client = match agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to access password vault agent: {err}");
            return ExitCode::from(1);
        }
    };

    match client.status() {
        Ok(status) => {
            log_debug!(
                "Password vault status: exists={}, unlocked={}, expires_in={:?}",
                status.vault_exists,
                status.unlocked,
                status.unlock_expires_in_seconds
            );
            println!("vault_exists: {}", status.vault_exists);
            println!("unlocked: {}", status.unlocked);
            if let Some(expires) = status.unlock_expires_in_seconds {
                println!("unlock_expires_in_seconds: {}", expires);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to read password vault status: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_set_master_password_cli() -> ExitCode {
    log_debug!("Running `cossh vault set-master-password`");
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    if let Some(mut password) = initial_password {
        password.zeroize();
        log_debug!("Password vault initialized with a new master password");
        println!("Password vault master password set");
        return ExitCode::SUCCESS;
    }

    let mut current_password = match prompt_existing_master_password_with_label("current") {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to capture current master password: {err}");
            return ExitCode::from(1);
        }
    };
    let mut new_password = match prompt_new_master_password_with_label("new") {
        Ok(password) => password,
        Err(err) => {
            current_password.zeroize();
            eprintln!("Failed to capture new master password: {err}");
            return ExitCode::from(1);
        }
    };

    let result = vault::rotate_master_password(&current_password, &new_password);
    current_password.zeroize();
    new_password.zeroize();

    match result {
        Ok(()) => {
            let _ = run_lock_cli();
            log_debug!("Password vault master password rotated successfully");
            println!("Password vault master password updated");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to rotate password vault master password: {err}");
            ExitCode::from(1)
        }
    }
}

pub fn run_internal_askpass() -> ExitCode {
    log_debug!("Handling internal askpass invocation");
    let Some(entry_name) = transport::internal_askpass_entry() else {
        eprintln!("Missing internal askpass entry");
        return ExitCode::from(1);
    };

    let client = match agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to access password vault agent: {err}");
            return ExitCode::from(1);
        }
    };

    let secret = match client.get_secret(&entry_name) {
        Ok(secret) => secret,
        Err(err) => {
            eprintln!("Failed to read password vault entry '{entry_name}': {err}");
            return ExitCode::from(1);
        }
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
        Err(err) => {
            eprintln!("Failed to write askpass response: {err}");
            ExitCode::from(1)
        }
    }
}

pub fn run_vault_command(vault_command: &args::VaultCommand) -> ExitCode {
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
