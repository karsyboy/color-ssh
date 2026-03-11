//! SSH command synthesis and vault-aware askpass wiring.

use super::DISABLE_VAULT_AUTOLOGIN_ENV;
use super::command_spec::{PreparedCommand, build_plain_ssh_command};
use super::vault::{VaultAccessError, query_vault_entry_status};
use crate::args;
use crate::args::validate_vault_entry_name;
use crate::auth::{agent, secret::ExposeSecret, transport};
use crate::config;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::log_debug;
use std::collections::HashSet;
use std::io;

const SSH_FLAGS_WITH_SEPARATE_VALUES: &[&str] = &[
    "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
];

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

fn mark_option_key(inspection: &mut SshArgInspection, key: &str) {
    inspection.option_keys.insert(key.to_ascii_lowercase());
}

fn mark_short_flag_override(arg: &str, inspection: &mut SshArgInspection) {
    for flag in arg.chars().skip(1) {
        match flag {
            'A' | 'a' => {
                inspection.has_forward_agent = true;
                mark_option_key(inspection, "ForwardAgent");
            }
            'C' => {
                mark_option_key(inspection, "Compression");
            }
            'f' => {
                mark_option_key(inspection, "ForkAfterAuthentication");
            }
            'g' => {
                mark_option_key(inspection, "GatewayPorts");
            }
            'M' => {
                mark_option_key(inspection, "ControlMaster");
            }
            'N' | 's' => {
                mark_option_key(inspection, "SessionType");
            }
            'n' => {
                mark_option_key(inspection, "StdinNull");
            }
            'q' | 'v' => {
                mark_option_key(inspection, "LogLevel");
            }
            'T' | 't' => {
                mark_option_key(inspection, "RequestTTY");
            }
            'X' | 'x' => {
                mark_option_key(inspection, "ForwardX11");
            }
            'Y' => {
                mark_option_key(inspection, "ForwardX11Trusted");
            }
            _ => {}
        }
    }
}

pub(crate) fn resolve_host_by_destination<'a>(destination: &str, hosts: &'a [InventoryHost]) -> Option<&'a InventoryHost> {
    if let Some(host) = hosts.iter().find(|host| host.name == destination) {
        return Some(host);
    }

    // Hostname lookup is only accepted when the match is unique.
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

    // Walk args once to identify destination position and caller overrides.
    for (idx, arg) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }

        match arg.as_str() {
            "-A" | "-a" => {
                inspection.has_forward_agent = true;
                mark_option_key(&mut inspection, "ForwardAgent");
                continue;
            }
            "-C" => {
                mark_option_key(&mut inspection, "Compression");
                continue;
            }
            "-f" => {
                mark_option_key(&mut inspection, "ForkAfterAuthentication");
                continue;
            }
            "-g" => {
                mark_option_key(&mut inspection, "GatewayPorts");
                continue;
            }
            "-l" => {
                inspection.has_user_flag = true;
                mark_option_key(&mut inspection, "User");
                skip_next = true;
                continue;
            }
            "-M" => {
                mark_option_key(&mut inspection, "ControlMaster");
                continue;
            }
            "-N" => {
                mark_option_key(&mut inspection, "SessionType");
                continue;
            }
            "-n" => {
                mark_option_key(&mut inspection, "StdinNull");
                continue;
            }
            "-p" => {
                inspection.has_port_flag = true;
                mark_option_key(&mut inspection, "Port");
                skip_next = true;
                continue;
            }
            "-P" => {
                mark_option_key(&mut inspection, "Tag");
                skip_next = true;
                continue;
            }
            "-q" => {
                mark_option_key(&mut inspection, "LogLevel");
                continue;
            }
            "-s" => {
                mark_option_key(&mut inspection, "SessionType");
                continue;
            }
            "-T" | "-t" => {
                mark_option_key(&mut inspection, "RequestTTY");
                continue;
            }
            "-X" | "-x" => {
                mark_option_key(&mut inspection, "ForwardX11");
                continue;
            }
            "-Y" => {
                mark_option_key(&mut inspection, "ForwardX11Trusted");
                continue;
            }
            "-i" => {
                inspection.has_identity_flag = true;
                mark_option_key(&mut inspection, "IdentityFile");
                skip_next = true;
                continue;
            }
            "-J" => {
                inspection.has_proxy_jump = true;
                mark_option_key(&mut inspection, "ProxyJump");
                skip_next = true;
                continue;
            }
            "-b" => {
                mark_option_key(&mut inspection, "BindAddress");
                skip_next = true;
                continue;
            }
            "-B" => {
                mark_option_key(&mut inspection, "BindInterface");
                skip_next = true;
                continue;
            }
            "-c" => {
                mark_option_key(&mut inspection, "Ciphers");
                skip_next = true;
                continue;
            }
            "-D" => {
                mark_option_key(&mut inspection, "DynamicForward");
                skip_next = true;
                continue;
            }
            "-I" => {
                mark_option_key(&mut inspection, "PKCS11Provider");
                skip_next = true;
                continue;
            }
            "-L" => {
                inspection.has_local_forward = true;
                mark_option_key(&mut inspection, "LocalForward");
                skip_next = true;
                continue;
            }
            "-m" => {
                mark_option_key(&mut inspection, "MACs");
                skip_next = true;
                continue;
            }
            "-R" => {
                inspection.has_remote_forward = true;
                mark_option_key(&mut inspection, "RemoteForward");
                skip_next = true;
                continue;
            }
            "-S" => {
                mark_option_key(&mut inspection, "ControlPath");
                skip_next = true;
                continue;
            }
            "-w" => {
                mark_option_key(&mut inspection, "Tunnel");
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
            mark_option_key(&mut inspection, "User");
            continue;
        }
        if arg.starts_with("-p") && arg.len() > 2 {
            inspection.has_port_flag = true;
            mark_option_key(&mut inspection, "Port");
            continue;
        }
        if arg.starts_with("-i") && arg.len() > 2 {
            inspection.has_identity_flag = true;
            mark_option_key(&mut inspection, "IdentityFile");
            continue;
        }
        if arg.starts_with("-J") && arg.len() > 2 {
            inspection.has_proxy_jump = true;
            mark_option_key(&mut inspection, "ProxyJump");
            continue;
        }
        if arg.starts_with("-L") && arg.len() > 2 {
            inspection.has_local_forward = true;
            mark_option_key(&mut inspection, "LocalForward");
            continue;
        }
        if arg.starts_with("-R") && arg.len() > 2 {
            inspection.has_remote_forward = true;
            mark_option_key(&mut inspection, "RemoteForward");
            continue;
        }
        if arg.starts_with("-o") && arg.len() > 2 {
            record_ssh_option(&arg[2..], &mut inspection);
            continue;
        }
        if arg.starts_with("-b") && arg.len() > 2 {
            mark_option_key(&mut inspection, "BindAddress");
            continue;
        }
        if arg.starts_with("-B") && arg.len() > 2 {
            mark_option_key(&mut inspection, "BindInterface");
            continue;
        }
        if arg.starts_with("-c") && arg.len() > 2 {
            mark_option_key(&mut inspection, "Ciphers");
            continue;
        }
        if arg.starts_with("-D") && arg.len() > 2 {
            mark_option_key(&mut inspection, "DynamicForward");
            continue;
        }
        if arg.starts_with("-I") && arg.len() > 2 {
            mark_option_key(&mut inspection, "PKCS11Provider");
            continue;
        }
        if arg.starts_with("-m") && arg.len() > 2 {
            mark_option_key(&mut inspection, "MACs");
            continue;
        }
        if arg.starts_with("-S") && arg.len() > 2 {
            mark_option_key(&mut inspection, "ControlPath");
            continue;
        }
        if arg.starts_with("-w") && arg.len() > 2 {
            mark_option_key(&mut inspection, "Tunnel");
            continue;
        }
        if arg.starts_with("-P") && arg.len() > 2 {
            mark_option_key(&mut inspection, "Tag");
            continue;
        }
        if arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2 {
            mark_short_flag_override(arg, &mut inspection);
        }

        if arg.starts_with('-') {
            if SSH_FLAGS_WITH_SEPARATE_VALUES.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }

        // First non-flag token is treated as destination.
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

fn configure_internal_askpass_for_entry(command: &mut PreparedCommand, pass_entry_name: &str) -> io::Result<()> {
    if !validate_vault_entry_name(pass_entry_name) {
        log_debug!("Resolved password vault entry name was invalid");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "password auto-login requires a valid vault entry name",
        ));
    }

    let client = agent::AgentClient::new().map_err(|err| io::Error::other(err.to_string()))?;
    let askpass_token = client.authorize_askpass(pass_entry_name).map_err(|err| {
        log_debug!("Failed to authorize internal askpass token: {}", err);
        io::Error::new(io::ErrorKind::PermissionDenied, format!("failed to authorize vault askpass token: {err}"))
    })?;

    if let Err(err) = transport::configure_internal_askpass_env(&mut command.env, askpass_token.expose_secret()) {
        log_debug!("Failed to configure internal askpass helper: {}", err);
        return Err(io::Error::other(format!("failed to configure internal askpass helper: {err}")));
    }

    Ok(())
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

    if !inspection.has_identity_flag && !host.ssh.identity_files.is_empty() {
        for identity_file in &host.ssh.identity_files {
            injected.push("-i".to_string());
            injected.push(identity_file.clone());
        }
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
        && let Some(forward_agent) = host.ssh.forward_agent.as_ref()
    {
        inject_ssh_option(&mut injected, "ForwardAgent", forward_agent.clone());
    }

    if !inspection.has_identities_only
        && let Some(identities_only) = host.ssh.identities_only
    {
        inject_ssh_option(&mut injected, "IdentitiesOnly", if identities_only { "yes" } else { "no" });
    }

    if !inspection.has_local_forward {
        for forward in &host.ssh.local_forward {
            injected.push("-L".to_string());
            injected.push(crate::inventory::normalize_ssh_forward_spec(forward));
        }
    }

    if !inspection.has_remote_forward {
        for forward in &host.ssh.remote_forward {
            injected.push("-R".to_string());
            injected.push(crate::inventory::normalize_ssh_forward_spec(forward));
        }
    }

    for (key, values) in &host.ssh.extra_options {
        let normalized_key = key.trim().to_ascii_lowercase();
        if inspection.option_keys.contains(&normalized_key)
            || matches!(
                normalized_key.as_str(),
                "user" | "port" | "identityfile" | "proxyjump" | "proxycommand" | "forwardagent" | "identitiesonly" | "localforward" | "remoteforward"
            )
        {
            continue;
        }
        for value in values {
            inject_ssh_option(&mut injected, key, value.clone());
        }
    }

    // Preserve user@host form when user is explicit in destination.
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

pub(crate) fn build_ssh_command(args: &[String], explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let destination = args::extract_destination_host(args);
    let inventory_hosts = crate::inventory::load_inventory_tree().ok().map(|tree| tree.hosts).unwrap_or_default();
    let resolved_host = destination
        .as_deref()
        .and_then(|destination| resolve_host_by_destination(destination, &inventory_hosts))
        .filter(|host| matches!(&host.protocol, ConnectionProtocol::Ssh))
        .cloned();

    let effective_args = resolved_host.as_ref().map_or_else(|| args.to_vec(), |host| synthesize_ssh_args(args, host));
    let mut command = build_plain_ssh_command(&effective_args);

    if std::env::var_os(DISABLE_VAULT_AUTOLOGIN_ENV).is_some() {
        log_debug!("Direct password auto-login disabled by environment override");
        return Ok(command);
    }

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
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "password auto-login requires a valid vault entry name",
        ));
    }

    let client = agent::AgentClient::new().map_err(|err| io::Error::other(err.to_string()))?;
    let entry_status = match query_vault_entry_status(&client, &pass_entry_name) {
        Ok(status) => status,
        Err(VaultAccessError::VaultNotInitialized) => {
            log_debug!("Password vault is not initialized during direct SSH launch");
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`",
            ));
        }
        Err(VaultAccessError::Query(err)) => {
            log_debug!("Password vault lookup failed during direct SSH launch: {}", err);
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, err));
        }
        Err(VaultAccessError::LockedWithoutTerminal) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault is locked; run `cossh vault unlock`",
            ));
        }
        Err(VaultAccessError::UnlockFailed(err)) => {
            log_debug!("Password vault unlock failed during direct SSH launch: {}", err);
            command.fallback_notice = Some(
                "Password auto-login is unavailable because vault unlock failed after multiple attempts; continuing with the standard SSH password prompt."
                    .to_string(),
            );
            return Ok(command);
        }
    };

    if !entry_status.exists {
        log_debug!("Password vault entry '{}' was not found", pass_entry_name);
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("password vault entry '{pass_entry_name}' was not found"),
        ));
    }

    configure_internal_askpass_for_entry(&mut command, &pass_entry_name)?;
    // At this point SSH can request password prompts through the internal helper.
    log_debug!("Configured internal askpass helper for direct SSH launch");
    Ok(command)
}

pub(crate) fn build_ssh_command_for_host(host: &InventoryHost, explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let effective_args = synthesize_ssh_args(std::slice::from_ref(&host.name), host);
    let mut command = build_plain_ssh_command(&effective_args);

    let Some(pass_entry_name) = explicit_pass_entry.map(|name| name.to_string()).or_else(|| host.vault_pass.clone()) else {
        return Ok(command);
    };

    configure_internal_askpass_for_entry(&mut command, &pass_entry_name)?;
    log_debug!("Configured internal askpass helper for TUI SSH host launch");
    Ok(command)
}

#[cfg(test)]
#[path = "../test/process/ssh_builder.rs"]
mod tests;
