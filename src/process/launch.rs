//! Command construction and process spawning for direct SSH and RDP launches.

use super::exit::map_exit_code;
use crate::args::RdpCommandArgs;
use crate::auth::{
    self, agent,
    ipc::UnlockPolicy,
    secret::{ExposeSecret, SensitiveString},
    transport,
};
use crate::command_path;
use crate::config;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::ssh_args;
use crate::validation::validate_vault_entry_name;
use crate::{Result, log_debug, log_debug_raw, log_error, log_info};
use std::collections::HashSet;
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::process::{Child, Command, ExitCode, Stdio};

const SSH_FLAGS_WITH_SEPARATE_VALUES: &[&str] = &[
    "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
];

#[derive(Debug)]
pub(crate) struct PreparedCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) stdin_payload: Option<SensitiveString>,
    pub(crate) fallback_notice: Option<String>,
}

impl PreparedCommand {
    fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            env: Vec::new(),
            stdin_payload: None,
            fallback_notice: None,
        }
    }
}

#[derive(Debug)]
enum VaultAccessError {
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

#[derive(Debug, Default)]
struct SshArgInspection {
    destination_index: Option<usize>,
    destination_host: Option<String>,
    explicit_destination_user: Option<String>,
    has_user_flag: bool,
    has_port_flag: bool,
    has_identity_flag: bool,
    has_proxy_jump: bool,
    has_proxy_command: bool,
    has_forward_agent: bool,
    has_identities_only: bool,
    has_local_forward: bool,
    has_remote_forward: bool,
    option_keys: HashSet<String>,
}

pub(crate) fn build_plain_ssh_command(args: &[String]) -> PreparedCommand {
    PreparedCommand::new("ssh", args.to_vec())
}

pub(crate) fn command_from_spec(spec: &PreparedCommand) -> io::Result<Command> {
    let program_path = command_path::resolve_known_command_path(&spec.program)?;
    let mut command = Command::new(&program_path);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    Ok(command)
}

pub(crate) fn resolve_host_by_destination<'a>(destination: &str, hosts: &'a [InventoryHost]) -> Option<&'a InventoryHost> {
    if let Some(host) = hosts.iter().find(|host| host.name == destination) {
        return Some(host);
    }

    let mut host_matches = hosts.iter().filter(|host| host.host == destination);
    let first = host_matches.next()?;
    if host_matches.next().is_some() {
        return None;
    }

    Some(first)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resolve_pass_entry_from_hosts(destination: &str, explicit_entry: Option<&str>, hosts: &[InventoryHost]) -> Option<String> {
    if let Some(explicit_entry) = explicit_entry {
        return Some(explicit_entry.to_string());
    }

    resolve_host_by_destination(destination, hosts).and_then(|host| host.vault_pass.clone())
}

fn inspect_ssh_args(args: &[String]) -> SshArgInspection {
    let mut inspection = SshArgInspection::default();
    let mut skip_next = false;

    for (idx, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }

        match arg.as_str() {
            "-l" => {
                inspection.has_user_flag = true;
                skip_next = true;
                continue;
            }
            "-p" => {
                inspection.has_port_flag = true;
                skip_next = true;
                continue;
            }
            "-i" => {
                inspection.has_identity_flag = true;
                skip_next = true;
                continue;
            }
            "-J" => {
                inspection.has_proxy_jump = true;
                skip_next = true;
                continue;
            }
            "-L" => {
                inspection.has_local_forward = true;
                skip_next = true;
                continue;
            }
            "-R" => {
                inspection.has_remote_forward = true;
                skip_next = true;
                continue;
            }
            "-o" => {
                if let Some(option_arg) = args.get(idx + 1) {
                    record_ssh_option(option_arg, &mut inspection);
                }
                skip_next = true;
                continue;
            }
            _ => {}
        }

        if arg.starts_with("-l") && arg.len() > 2 {
            inspection.has_user_flag = true;
            continue;
        }
        if arg.starts_with("-p") && arg.len() > 2 {
            inspection.has_port_flag = true;
            continue;
        }
        if arg.starts_with("-i") && arg.len() > 2 {
            inspection.has_identity_flag = true;
            continue;
        }
        if arg.starts_with("-J") && arg.len() > 2 {
            inspection.has_proxy_jump = true;
            continue;
        }
        if arg.starts_with("-L") && arg.len() > 2 {
            inspection.has_local_forward = true;
            continue;
        }
        if arg.starts_with("-R") && arg.len() > 2 {
            inspection.has_remote_forward = true;
            continue;
        }
        if arg.starts_with("-o") && arg.len() > 2 {
            record_ssh_option(&arg[2..], &mut inspection);
            continue;
        }

        if arg.starts_with('-') {
            if SSH_FLAGS_WITH_SEPARATE_VALUES.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }

        inspection.destination_index = Some(idx);
        inspection.destination_host = Some(
            arg.split_once('@')
                .map(|(user, host)| {
                    if !user.is_empty() {
                        inspection.explicit_destination_user = Some(user.to_string());
                    }
                    host.to_string()
                })
                .unwrap_or_else(|| arg.clone()),
        );
        break;
    }

    inspection
}

fn record_ssh_option(option_arg: &str, inspection: &mut SshArgInspection) {
    let option_key = option_arg.split_once('=').map(|(key, _)| key).unwrap_or(option_arg).trim().to_ascii_lowercase();
    if option_key.is_empty() {
        return;
    }

    inspection.option_keys.insert(option_key.clone());
    match option_key.as_str() {
        "user" => inspection.has_user_flag = true,
        "port" => inspection.has_port_flag = true,
        "identityfile" => inspection.has_identity_flag = true,
        "proxyjump" => inspection.has_proxy_jump = true,
        "proxycommand" => inspection.has_proxy_command = true,
        "forwardagent" => inspection.has_forward_agent = true,
        "identitiesonly" => inspection.has_identities_only = true,
        "localforward" => inspection.has_local_forward = true,
        "remoteforward" => inspection.has_remote_forward = true,
        _ => {}
    }
}

fn inject_ssh_option(args: &mut Vec<String>, key: &str, value: impl Into<String>) {
    args.push("-o".to_string());
    args.push(format!("{key}={}", value.into()));
}

pub(crate) fn synthesize_ssh_args(args: &[String], host: &InventoryHost) -> Vec<String> {
    let inspection = inspect_ssh_args(args);
    let Some(destination_index) = inspection.destination_index else {
        return args.to_vec();
    };

    let mut injected = Vec::new();

    if !inspection.has_user_flag
        && inspection.explicit_destination_user.is_none()
        && let Some(user) = host.user.as_ref().filter(|value| !value.trim().is_empty())
    {
        injected.push("-l".to_string());
        injected.push(user.clone());
    }

    if !inspection.has_port_flag
        && let Some(port) = host.port
    {
        injected.push("-p".to_string());
        injected.push(port.to_string());
    }

    if !inspection.has_identity_flag
        && let Some(identity_file) = host.ssh.identity_file.as_ref()
    {
        injected.push("-i".to_string());
        injected.push(identity_file.clone());
    }

    if !inspection.has_proxy_jump
        && let Some(proxy_jump) = host.ssh.proxy_jump.as_ref()
    {
        inject_ssh_option(&mut injected, "ProxyJump", proxy_jump.clone());
    }

    if !inspection.has_proxy_command
        && let Some(proxy_command) = host.ssh.proxy_command.as_ref()
    {
        inject_ssh_option(&mut injected, "ProxyCommand", proxy_command.clone());
    }

    if !inspection.has_forward_agent
        && let Some(forward_agent) = host.ssh.forward_agent
    {
        inject_ssh_option(&mut injected, "ForwardAgent", if forward_agent { "yes" } else { "no" });
    }

    if !inspection.has_identities_only
        && let Some(identities_only) = host.ssh.identities_only
    {
        inject_ssh_option(&mut injected, "IdentitiesOnly", if identities_only { "yes" } else { "no" });
    }

    if !inspection.has_local_forward {
        for forward in &host.ssh.local_forward {
            injected.push("-L".to_string());
            injected.push(forward.clone());
        }
    }

    if !inspection.has_remote_forward {
        for forward in &host.ssh.remote_forward {
            injected.push("-R".to_string());
            injected.push(forward.clone());
        }
    }

    for (key, value) in &host.ssh.extra_options {
        let normalized_key = key.trim().to_ascii_lowercase();
        if inspection.option_keys.contains(&normalized_key)
            || matches!(
                normalized_key.as_str(),
                "user" | "port" | "identityfile" | "proxyjump" | "proxycommand" | "forwardagent" | "identitiesonly" | "localforward" | "remoteforward"
            )
        {
            continue;
        }
        inject_ssh_option(&mut injected, key, value.clone());
    }

    let destination = if let Some(explicit_user) = inspection.explicit_destination_user {
        format!("{explicit_user}@{}", host.host)
    } else {
        host.host.clone()
    };

    let mut effective_args = Vec::with_capacity(args.len() + injected.len());
    effective_args.extend_from_slice(&args[..destination_index]);
    effective_args.extend(injected);
    effective_args.push(destination);
    effective_args.extend_from_slice(&args[destination_index + 1..]);
    effective_args
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

fn query_vault_entry_status(client: &agent::AgentClient, pass_entry_name: &str) -> std::result::Result<agent::AgentEntryStatus, VaultAccessError> {
    let entry_status = match client.entry_status(pass_entry_name) {
        Ok(status) => status,
        Err(agent::AgentError::VaultNotInitialized) => return Err(VaultAccessError::VaultNotInitialized),
        Err(err) => return Err(VaultAccessError::Query(err.to_string())),
    };

    if !entry_status.status.unlocked {
        log_debug!("Password vault was locked during launch preparation");
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

fn resolve_vault_password(pass_entry_name: &str) -> io::Result<SensitiveString> {
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

    let askpass_token = client
        .authorize_askpass(pass_entry_name)
        .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;

    client
        .get_secret(askpass_token.expose_secret())
        .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))
}

pub(crate) fn build_ssh_command(args: &[String], explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let destination = ssh_args::extract_destination_host(args);
    let inventory_hosts = crate::inventory::load_inventory_tree().ok().map(|tree| tree.hosts).unwrap_or_default();
    let resolved_host = destination
        .as_deref()
        .and_then(|destination| resolve_host_by_destination(destination, &inventory_hosts))
        .filter(|host| matches!(&host.protocol, ConnectionProtocol::Ssh))
        .cloned();

    let effective_args = resolved_host.as_ref().map_or_else(|| args.to_vec(), |host| synthesize_ssh_args(args, host));
    let mut command = build_plain_ssh_command(&effective_args);

    let auth_settings = config::auth_settings();
    if !auth_settings.direct_password_autologin {
        log_debug!("Direct password auto-login disabled in auth settings");
        return Ok(command);
    }

    let pass_entry_source = if explicit_pass_entry.is_some() {
        "direct override"
    } else {
        "inventory lookup"
    };
    let Some(pass_entry_name) = explicit_pass_entry
        .map(|name| name.to_string())
        .or_else(|| resolved_host.as_ref().and_then(|host| host.vault_pass.clone()))
    else {
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
    let entry_status = match query_vault_entry_status(&client, &pass_entry_name) {
        Ok(status) => status,
        Err(VaultAccessError::VaultNotInitialized) => {
            log_debug!("Password vault is not initialized during direct SSH launch");
            command.fallback_notice = Some(
                "Password auto-login is unavailable because the password vault is not initialized; continuing with the standard SSH password prompt."
                    .to_string(),
            );
            return Ok(command);
        }
        Err(VaultAccessError::Query(err)) => {
            log_debug!("Password vault lookup failed during direct SSH launch: {}", err);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because the password vault could not be queried ({err}); continuing with the standard SSH password prompt."
            ));
            return Ok(command);
        }
        Err(VaultAccessError::LockedWithoutTerminal) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault is locked; run `cossh vault unlock`",
            ));
        }
        Err(VaultAccessError::UnlockFailed(err)) => {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, err));
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

fn rdp_server_address(host: &InventoryHost) -> String {
    let destination = host.host.as_str();
    match host.port {
        Some(port) if destination.contains(':') && !destination.starts_with('[') => format!("[{destination}]:{port}"),
        Some(port) => format!("{destination}:{port}"),
        None => destination.to_string(),
    }
}

fn has_rdp_cert_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg.starts_with("/cert:") || arg == "/cert")
}

fn build_rdp_stdin_payload(host: &InventoryHost, password: SensitiveString) -> io::Result<SensitiveString> {
    let Some(user) = host.user.as_deref().filter(|value| !value.trim().is_empty()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP username is required; set `user` in the inventory or pass `--user`",
        ));
    };

    let server = rdp_server_address(host);
    let mut args = Vec::with_capacity(host.rdp.args.len() + 6);
    args.push(format!("/u:{user}"));
    if let Some(domain) = host.rdp.domain.as_deref().filter(|value| !value.trim().is_empty()) {
        args.push(format!("/d:{domain}"));
    }
    args.push(format!("/v:{server}"));
    args.push(format!("/p:{}", password.expose_secret()));
    if !has_rdp_cert_flag(&host.rdp.args) {
        args.push("/cert:tofu".to_string());
    }
    args.extend(host.rdp.args.iter().cloned());

    Ok(SensitiveString::from(args.join("\n") + "\n"))
}

pub(crate) fn build_rdp_command_for_host(host: &InventoryHost, explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let pass_entry_name = explicit_pass_entry.map(str::to_string).or_else(|| host.vault_pass.clone()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP launch requires a password vault entry; set `vault_pass` in the inventory or pass `--pass-entry`",
        )
    })?;

    let password = resolve_vault_password(&pass_entry_name)?;
    let stdin_payload = build_rdp_stdin_payload(host, password)?;
    let mut command = PreparedCommand::new("xfreerdp", vec!["/args-from:stdin".to_string()]);
    command.stdin_payload = Some(stdin_payload);
    Ok(command)
}

pub(crate) fn build_rdp_command(args: &RdpCommandArgs, explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let configured_host = crate::inventory::load_inventory_tree()
        .ok()
        .and_then(|tree| resolve_host_by_destination(&args.target, &tree.hosts).cloned());

    let mut host = configured_host.unwrap_or_else(|| {
        let mut host = InventoryHost::new(args.target.clone());
        host.host = args.target.clone();
        host
    });

    host.protocol = ConnectionProtocol::Rdp;
    if let Some(user) = args.user.as_ref() {
        host.user = Some(user.clone());
    }
    if let Some(domain) = args.domain.as_ref() {
        host.rdp.domain = Some(domain.clone());
    }
    if let Some(port) = args.port {
        host.port = Some(port);
    }
    host.rdp.args.extend(args.extra_args.iter().cloned());

    build_rdp_command_for_host(&host, explicit_pass_entry)
}

fn write_stdin_payload(child: &mut Child, stdin_payload: SensitiveString) -> io::Result<()> {
    let Some(mut stdin) = child.stdin.take() else {
        return Ok(());
    };

    stdin.write_all(stdin_payload.expose_secret().as_bytes())?;
    stdin.flush()?;
    Ok(())
}

pub(crate) fn spawn_command(command_spec: PreparedCommand, stdout: Stdio, stderr: Stdio) -> io::Result<Child> {
    log_debug!(
        "Spawning {}: arg_count={} env_override_count={} stdin_payload={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len(),
        command_spec.stdin_payload.is_some()
    );
    log_debug_raw!("Spawning {} with args: {:?}", command_spec.program, command_spec.args);

    let stdin_mode = if command_spec.stdin_payload.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    };

    let mut child = command_from_spec(&command_spec)?
        .stdin(stdin_mode)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|err| {
            log_error!("Failed to spawn {} command: {}", command_spec.program, err);
            err
        })?;

    if let Some(stdin_payload) = command_spec.stdin_payload
        && let Err(err) = write_stdin_payload(&mut child, stdin_payload)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }

    log_debug!("{} process spawned (PID: {:?})", command_spec.program, child.id());
    Ok(child)
}

pub(crate) fn spawn_passthrough(command_spec: PreparedCommand) -> Result<ExitCode> {
    log_debug!(
        "Spawning {} in passthrough mode: arg_count={} env_override_count={} stdin_payload={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len(),
        command_spec.stdin_payload.is_some()
    );
    log_debug_raw!("Spawning {} in passthrough mode with args: {:?}", command_spec.program, command_spec.args);

    let mut child = spawn_command(command_spec, Stdio::inherit(), Stdio::inherit()).map_err(|err| {
        log_error!("Failed to execute command in passthrough mode: {}", err);
        err
    })?;

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for passthrough process: {}", err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("Passthrough process exited with code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}
