use cossh::auth::{agent, ipc::UnlockPolicy, vault};
use cossh::{Result, args, config, log, log_debug, log_error, log_info, process, tui};
use std::process::ExitCode;
use zeroize::Zeroize;

const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Extracts the SSH destination hostname from the provided SSH arguments returns hostname or none
fn extract_ssh_destination(ssh_args: &[String]) -> Option<String> {
    // SSH flags that take an argument based off ssh version "OpenSSH_10.2p1"
    let flags_with_args = [
        "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
    ];

    let mut skip_next = false;

    for arg in ssh_args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg.starts_with('-') {
            if flags_with_args.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }

        // Extract just the hostname part after @ if it exist
        return Some(arg.split_once('@').map_or_else(|| arg.clone(), |(_, host)| host.to_string()));
    }

    None
}

fn resolve_logging_settings(args: &args::MainArgs, debug_from_config: bool, ssh_log_from_config: bool) -> (bool, bool) {
    if args.test_mode {
        (args.debug, args.ssh_logging)
    } else {
        (args.debug || debug_from_config, args.ssh_logging || ssh_log_from_config)
    }
}

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
    let password = rpassword::prompt_password(&format!("Enter {label} vault master password: ")).map_err(|err| err.to_string())?;
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

fn unlock_policy_from_config() -> UnlockPolicy {
    let auth_settings = config::auth_settings();
    UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
}

fn initialize_vault_if_needed() -> std::result::Result<Option<String>, String> {
    if vault::vault_exists().map_err(|err| err.to_string())? {
        return Ok(None);
    }

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

fn run_vault_init_cli() -> ExitCode {
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
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let mut master_password = match initial_password {
        Some(password) => password,
        None => match prompt_existing_master_password() {
            Ok(password) => password,
            Err(err) => {
                eprintln!("Failed to unlock password vault: {err}");
                return ExitCode::from(1);
            }
        },
    };

    let unlocked = match vault::unlock_with_password(&master_password) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            master_password.zeroize();
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };
    master_password.zeroize();

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
    match vault::vault_exists() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("Password vault is not initialized. Run `cossh vault init` first.");
            return ExitCode::from(1);
        }
        Err(err) => {
            eprintln!("Failed to read password vault state: {err}");
            return ExitCode::from(1);
        }
    }

    let mut master_password = match prompt_existing_master_password() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };
    let unlocked = match vault::unlock_with_password(&master_password) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            master_password.zeroize();
            eprintln!("Failed to unlock password vault: {err}");
            return ExitCode::from(1);
        }
    };
    master_password.zeroize();

    match unlocked.remove_entry(pass_name) {
        Ok(()) => {
            println!("Removed password vault entry: {}", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to remove password vault entry: {err}");
            ExitCode::from(1)
        }
    }
}

fn run_unlock_cli() -> ExitCode {
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    let mut master_password = match initial_password {
        Some(password) => password,
        None => match prompt_existing_master_password() {
            Ok(password) => password,
            Err(err) => {
                eprintln!("Failed to unlock password vault: {err}");
                return ExitCode::from(1);
            }
        },
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
    match vault::vault_exists() {
        Ok(true) => {}
        Ok(false) => {
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
            println!("Password vault locked");
            ExitCode::SUCCESS
        }
        Err(agent::AgentError::Io(_)) => {
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
    let client = match agent::AgentClient::new() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("Failed to access password vault agent: {err}");
            return ExitCode::from(1);
        }
    };

    match client.status() {
        Ok(status) => {
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
    let initial_password = match initialize_vault_if_needed() {
        Ok(password) => password,
        Err(err) => {
            eprintln!("Failed to initialize password vault: {err}");
            return ExitCode::from(1);
        }
    };

    if let Some(mut password) = initial_password {
        password.zeroize();
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
            println!("Password vault master password updated");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to rotate password vault master password: {err}");
            ExitCode::from(1)
        }
    }
}

fn main() -> Result<ExitCode> {
    let args = args::main_args();

    let logger = log::Logger::new();

    // Enable debug logging only when explicitly requested on CLI.
    // Config-based debug mode is applied after loading config.
    if args.debug {
        logger.enable_debug();
    }
    log_info!("color-ssh {} starting", APP_VERSION);

    if args.agent_serve {
        agent::run_server().map_err(|err| {
            log_error!("Password vault agent failed: {}", err);
            std::io::Error::other(err.to_string())
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(vault_command) = args.vault_command.as_ref() {
        let exit_code = match vault_command {
            args::VaultCommand::Init => run_vault_init_cli(),
            args::VaultCommand::AddPass(pass_name) => run_add_pass_cli(pass_name),
            args::VaultCommand::RemovePass(pass_name) => run_remove_pass_cli(pass_name),
            args::VaultCommand::Unlock => run_unlock_cli(),
            args::VaultCommand::Lock => run_lock_cli(),
            args::VaultCommand::Status => run_vault_status_cli(),
            args::VaultCommand::SetMasterPassword => run_set_master_password_cli(),
        };
        return Ok(exit_code);
    }

    // If interactive mode is requested, launch the session manager
    if args.interactive {
        log_info!("Launching interactive session manager");
        // Init config so session manager can read interactive settings (if configured)
        let debug_from_config = if config::init_session_config(args.profile.clone()).is_ok() {
            match config::get_config().read() {
                Ok(config_guard) => config_guard.settings.debug_mode,
                Err(poisoned) => {
                    log_error!("Configuration lock poisoned while reading interactive debug setting; continuing with recovered state");
                    poisoned.into_inner().settings.debug_mode
                }
            }
        } else {
            false
        };
        let (final_debug, _) = resolve_logging_settings(&args, debug_from_config, false);
        if final_debug {
            if !logger.is_debug_enabled() {
                logger.enable_debug();
            }
            if args.debug {
                log_debug!("Debug mode enabled via CLI argument");
            } else {
                log_debug!("Debug mode enabled via config file");
            }
        } else {
            logger.disable_debug();
        }

        if let Err(err) = tui::run_session_manager() {
            eprintln!("Session manager error: {err}");
            let _ = logger.flush_debug();
            std::process::exit(1);
        }
        let _ = logger.flush_debug();
        return Ok(ExitCode::SUCCESS);
    }

    if let Err(err) = config::init_session_config(args.profile.clone()) {
        eprintln!("Failed to initialize config: {err}");
        let _ = logger.flush_debug();
        std::process::exit(1);
    }

    // Get global settings from config
    let (debug_from_config, ssh_log_from_config, show_title) = {
        match config::get_config().read() {
            Ok(config_guard) => (
                config_guard.settings.debug_mode,
                config_guard.settings.ssh_logging,
                config_guard.settings.show_title,
            ),
            Err(poisoned) => {
                log_error!("Configuration lock poisoned while reading global settings; continuing with recovered state");
                let config_guard = poisoned.into_inner();
                (
                    config_guard.settings.debug_mode,
                    config_guard.settings.ssh_logging,
                    config_guard.settings.show_title,
                )
            }
        }
    };

    // Determine final logging mode
    let (final_debug, final_ssh_log) = resolve_logging_settings(&args, debug_from_config, ssh_log_from_config);

    if final_debug {
        if !logger.is_debug_enabled() {
            logger.enable_debug();
        }
        if args.debug {
            log_debug!("Debug mode enabled via CLI argument");
        } else {
            log_debug!("Debug mode enabled via config file");
        }
    } else {
        log_debug!("Debug mode not requested, disabling after initial config load");
        logger.disable_debug();
    }

    // Enable SSH logging
    if final_ssh_log {
        logger.enable_ssh_logging();
        if args.ssh_logging {
            log_info!("SSH logging enabled via CLI argument");
        } else {
            log_info!("SSH logging enabled via config file");
        }
    }

    log_debug!("Parsed arguments: {:?}", args);

    if show_title {
        log_debug!("Banner display enabled in config, printing banner");
        let title = [
            " ",
            "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
            "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
            "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
            "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
            "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
            "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
            concat!(
                "\x1b[31mVersion: \x1b[33mv",
                env!("CARGO_PKG_VERSION"),
                "\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m"
            ),
            " ",
        ];

        for line in &title {
            println!("{line}\x1b[0m");
        }
    }

    // Configure SSH session logging
    if logger.is_ssh_logging_enabled() {
        let session_hostname = extract_ssh_destination(&args.ssh_args).unwrap_or_else(|| "unknown".to_string());

        // Use COSSH_SESSION_NAME env var if set (from session manager tabs), otherwise use hostname
        let session_name = std::env::var("COSSH_SESSION_NAME").unwrap_or_else(|_| session_hostname.clone());
        let session_name = log::sanitize_session_name(&session_name);
        match config::get_config().write() {
            Ok(mut config_guard) => {
                config_guard.metadata.session_name = session_name.clone();
            }
            Err(poisoned) => {
                log_error!("Configuration lock poisoned while setting session name; continuing with recovered state");
                let mut config_guard = poisoned.into_inner();
                config_guard.metadata.session_name = session_name.clone();
            }
        }
        log_debug!("Session name set to: {session_name}");
    }

    // Start the config file watcher in the background
    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    // Start the SSH process
    log_info!("Launching SSH process handler");
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive, args.pass_entry).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        let _ = logger.flush_debug();
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    let _ = logger.flush_debug();
    Ok(exit_code)
}

#[cfg(test)]
#[path = "test/main.rs"]
mod tests;
