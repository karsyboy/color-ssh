//! SSH command construction and process spawning.

use super::exit::map_exit_code;
use crate::auth::{self, agent, ipc::UnlockPolicy, secret::ExposeSecret, transport};
use crate::command_path;
use crate::config;
use crate::ssh_args;
use crate::ssh_config::SshHost;
use crate::validation::validate_vault_entry_name;
use crate::{Result, log_debug, log_debug_raw, log_error, log_info};
use std::io::{self, IsTerminal};
use std::process::{Child, Command, ExitCode, Stdio};

#[derive(Debug)]
pub(super) struct PreparedSshCommand {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) fallback_notice: Option<String>,
}

pub(super) fn build_plain_ssh_command(args: &[String]) -> PreparedSshCommand {
    PreparedSshCommand {
        program: "ssh".to_string(),
        args: args.to_vec(),
        env: Vec::new(),
        fallback_notice: None,
    }
}

fn command_from_spec(spec: &PreparedSshCommand) -> io::Result<Command> {
    let program_path = command_path::resolve_known_command_path(&spec.program)?;
    let mut command = Command::new(&program_path);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    Ok(command)
}

pub(super) fn resolve_pass_entry_from_hosts(destination: &str, explicit_entry: Option<&str>, hosts: &[SshHost]) -> Option<String> {
    if let Some(explicit_entry) = explicit_entry {
        return Some(explicit_entry.to_string());
    }

    if let Some(host) = hosts.iter().find(|host| host.name == destination && host.pass_key.is_some()) {
        return host.pass_key.clone();
    }

    let mut hostname_matches = hosts
        .iter()
        .filter(|host| host.hostname.as_deref() == Some(destination) && host.pass_key.is_some());
    let first = hostname_matches.next()?;
    if hostname_matches.next().is_some() {
        return None;
    }

    first.pass_key.clone()
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

pub(super) fn build_ssh_command(args: &[String], explicit_pass_entry: Option<&str>) -> io::Result<PreparedSshCommand> {
    let mut command = build_plain_ssh_command(args);
    let auth_settings = config::auth_settings();
    if !auth_settings.direct_password_autologin {
        log_debug!("Direct password auto-login disabled in auth settings");
        return Ok(command);
    }

    let pass_entry_source = if explicit_pass_entry.is_some() {
        "direct override"
    } else {
        "ssh config lookup"
    };
    let Some(pass_entry_name) = explicit_pass_entry.map(|name| name.to_string()).or_else(|| {
        let destination = ssh_args::extract_destination_host(args)?;
        let hosts = crate::ssh_config::load_ssh_host_tree().ok()?.hosts;
        resolve_pass_entry_from_hosts(&destination, None, &hosts)
    }) else {
        log_debug!("No password vault entry resolved for direct SSH launch");
        return Ok(command);
    };
    log_debug!("Resolved password vault entry for direct SSH launch via {}", pass_entry_source);

    if !validate_vault_entry_name(&pass_entry_name) {
        log_debug!("Resolved password vault entry name was invalid");
        command.fallback_notice = Some(
            "Password auto-login is unavailable because the requested password entry name is invalid; continuing with the standard SSH password prompt."
                .to_string(),
        );
        return Ok(command);
    }

    let client = agent::AgentClient::new().map_err(|err| io::Error::other(err.to_string()))?;
    let entry_status = match client.entry_status(&pass_entry_name) {
        Ok(status) => status,
        Err(agent::AgentError::VaultNotInitialized) => {
            log_debug!("Password vault is not initialized during direct SSH launch");
            command.fallback_notice = Some(
                "Password auto-login is unavailable because the password vault is not initialized; continuing with the standard SSH password prompt."
                    .to_string(),
            );
            return Ok(command);
        }
        Err(err) => {
            log_debug!("Password vault lookup failed during direct SSH launch: {}", err);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because the password vault could not be queried ({err}); continuing with the standard SSH password prompt."
            ));
            return Ok(command);
        }
    };

    if !entry_status.exists {
        log_debug!("Password vault entry '{}' was not found", pass_entry_name);
        command.fallback_notice = Some(format!(
            "Password auto-login is unavailable because vault entry '{}' was not found; continuing with the standard SSH password prompt.",
            pass_entry_name
        ));
        return Ok(command);
    }

    if !entry_status.status.unlocked {
        log_debug!("Password vault was locked during direct SSH launch");
        if !io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault is locked; run `cossh vault unlock`",
            ));
        }
        unlock_agent_interactively(&client)?;
        log_debug!("Retrying password vault entry lookup after unlock");
        let entry_status = client
            .entry_status(&pass_entry_name)
            .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;
        if !entry_status.exists {
            log_debug!("Password vault entry '{}' was not found after unlock", pass_entry_name);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because vault entry '{}' was not found; continuing with the standard SSH password prompt.",
                pass_entry_name
            ));
            return Ok(command);
        }
        if !entry_status.status.unlocked {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault remains locked after unlock attempt",
            ));
        }
    }

    let askpass_token = match client.authorize_askpass(&pass_entry_name) {
        Ok(token) => token,
        Err(err) => {
            log_debug!("Failed to authorize internal askpass token: {}", err);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because a vault access token could not be issued ({err}); continuing with the standard SSH password prompt."
            ));
            return Ok(command);
        }
    };

    if let Err(err) = transport::configure_internal_askpass_env(&mut command.env, askpass_token.expose_secret()) {
        log_debug!("Failed to configure internal askpass helper: {}", err);
        command.fallback_notice = Some(format!(
            "Password auto-login is unavailable because the internal askpass helper could not be configured ({err}); continuing with the standard SSH password prompt."
        ));
        return Ok(command);
    }
    log_debug!("Configured internal askpass helper for direct SSH launch");
    Ok(command)
}

/// Spawns an SSH process with the provided arguments and returns the child process.
pub(super) fn spawn_ssh(command_spec: &PreparedSshCommand) -> io::Result<Child> {
    log_debug!(
        "Spawning {}: arg_count={} env_override_count={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len()
    );
    log_debug_raw!("Spawning {} with args: {:?}", command_spec.program, command_spec.args);

    let mut command = command_from_spec(command_spec)?;
    command.stdin(Stdio::inherit()).stdout(Stdio::piped()).stderr(Stdio::inherit());

    let child = command.spawn().map_err(|err| {
        log_error!("Failed to spawn SSH command: {}", err);
        err
    })?;

    log_debug!("SSH process spawned (PID: {:?})", child.id());
    Ok(child)
}

/// Spawns SSH for non-interactive commands with direct stdout passthrough.
pub(super) fn spawn_ssh_passthrough(command_spec: &PreparedSshCommand) -> Result<ExitCode> {
    log_debug!(
        "Spawning {} in passthrough mode: arg_count={} env_override_count={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len()
    );
    log_debug_raw!("Spawning {} in passthrough mode with args: {:?}", command_spec.program, command_spec.args);

    let mut child = command_from_spec(command_spec)?
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| {
            log_error!("Failed to execute SSH command in passthrough mode: {}", err);
            err
        })?;

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for SSH passthrough process: {}", err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("SSH passthrough process exited with code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}
