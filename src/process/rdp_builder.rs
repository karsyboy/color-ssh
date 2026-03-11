//! RDP command construction and inventory/default resolution.

use super::command_spec::PreparedCommand;
use super::ssh_builder::resolve_host_by_destination;
use super::vault::resolve_vault_password_with_policy;
use crate::args::RdpCommandArgs;
use crate::auth::ipc::UnlockPolicy;
use crate::auth::secret::{ExposeSecret, SensitiveString};
use crate::config::AuthSettings;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use std::io::{self, IsTerminal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RdpLaunchMode {
    Pty,
    CapturedOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RdpCredentialSource {
    NativePrompt,
    VaultEntry,
    ManualPrompt,
}

#[derive(Debug)]
pub(crate) struct PreparedRdpLaunch {
    pub(crate) command: PreparedCommand,
    pub(crate) launch_mode: RdpLaunchMode,
    pub(crate) credential_source: RdpCredentialSource,
}

impl PreparedRdpLaunch {
    pub(crate) fn into_command(self) -> PreparedCommand {
        self.command
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RdpAuthMode {
    NativePrompt,
    SuppliedPassword { password: SensitiveString, source: RdpCredentialSource },
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

fn rdp_prompt_fallback_notice(detail: impl Into<String>) -> String {
    format!(
        "Password auto-login is unavailable because {}; continuing with the FreeRDP password prompt.",
        detail.into()
    )
}

fn build_rdp_args(host: &InventoryHost, password: Option<&SensitiveString>) -> io::Result<Vec<String>> {
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
    if let Some(password) = password {
        args.push(format!("/p:{}", password.expose_secret()));
    } else {
        // Force a terminal-backed credential prompt when vault auto-login is unavailable.
        args.push("+force-console-callbacks".to_string());
        args.push("/from-stdin:force".to_string());
    }
    // Keep default cert handling explicit when caller did not specify one.
    if !has_rdp_cert_flag(&host.rdp.args) {
        args.push("/cert:tofu".to_string());
    }
    args.extend(host.rdp.args.iter().cloned());

    Ok(args)
}

fn build_rdp_stdin_payload(args: &[String]) -> SensitiveString {
    SensitiveString::from(args.join("\n"))
}

fn terminal_prompting_available() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

fn prompt_rdp_username(host: &InventoryHost) -> io::Result<String> {
    let response = crate::auth::prompt_visible_value(&format!("Enter RDP username for {}: ", host.host))?;
    let username = response.trim();
    if username.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "RDP username cannot be empty"));
    }

    Ok(username.to_string())
}

fn prompt_rdp_password(host: &InventoryHost) -> io::Result<SensitiveString> {
    let prompt_target = host
        .user
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|user| format!("{user}@{}", host.host))
        .unwrap_or_else(|| host.host.clone());
    let password = crate::auth::prompt_hidden_secret(&format!("Enter RDP password for {prompt_target}: "))?;
    if password.expose_secret().is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "RDP password cannot be empty"));
    }

    Ok(password)
}

fn configured_rdp_host_for_args(args: &RdpCommandArgs) -> InventoryHost {
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

    host
}

fn resolve_rdp_auth_mode_with<F>(host: &InventoryHost, explicit_pass_entry: Option<&str>, resolve_password: F) -> (RdpAuthMode, Option<String>)
where
    F: FnOnce(&str) -> Result<SensitiveString, String>,
{
    let Some(pass_entry_name) = explicit_pass_entry.map(str::to_string).or_else(|| host.vault_pass.clone()) else {
        return (RdpAuthMode::NativePrompt, None);
    };

    match resolve_password(&pass_entry_name) {
        Ok(password) => (
            RdpAuthMode::SuppliedPassword {
                password,
                source: RdpCredentialSource::VaultEntry,
            },
            None,
        ),
        Err(err) => (RdpAuthMode::NativePrompt, Some(rdp_prompt_fallback_notice(err))),
    }
}

fn build_prepared_rdp_command(host: &InventoryHost, auth_mode: RdpAuthMode, fallback_notice: Option<String>) -> io::Result<PreparedRdpLaunch> {
    match auth_mode {
        RdpAuthMode::NativePrompt => {
            let mut command = PreparedCommand::new("xfreerdp", build_rdp_args(host, None)?);
            command.fallback_notice = fallback_notice;
            Ok(PreparedRdpLaunch {
                command,
                launch_mode: RdpLaunchMode::Pty,
                credential_source: RdpCredentialSource::NativePrompt,
            })
        }
        RdpAuthMode::SuppliedPassword { password, source } => {
            let stdin_args = build_rdp_args(host, Some(&password))?;
            let mut command = PreparedCommand::new("xfreerdp", vec!["/args-from:stdin".to_string()]);
            command.stdin_payload = Some(build_rdp_stdin_payload(&stdin_args));
            command.fallback_notice = fallback_notice;
            Ok(PreparedRdpLaunch {
                command,
                launch_mode: RdpLaunchMode::CapturedOutput,
                credential_source: source,
            })
        }
    }
}

fn resolve_direct_rdp_auth_mode_with<FPrompt>(
    host: &InventoryHost,
    explicit_pass_entry: Option<&str>,
    auth_settings: &AuthSettings,
    terminal_prompting_enabled: bool,
    prompt_password: &mut FPrompt,
) -> io::Result<(RdpAuthMode, Option<String>)>
where
    FPrompt: FnMut(&InventoryHost) -> io::Result<SensitiveString>,
{
    if explicit_pass_entry.is_some() || host.vault_pass.is_some() {
        let unlock_policy = UnlockPolicy::new(auth_settings.idle_timeout_seconds, auth_settings.session_timeout_seconds);
        return Ok(resolve_rdp_auth_mode_with(host, explicit_pass_entry, |pass_entry_name| {
            resolve_vault_password_with_policy(pass_entry_name, unlock_policy.clone()).map_err(|err| err.to_string())
        }));
    }

    if terminal_prompting_enabled {
        let password = prompt_password(host)?;
        return Ok((
            RdpAuthMode::SuppliedPassword {
                password,
                source: RdpCredentialSource::ManualPrompt,
            },
            None,
        ));
    }

    Ok((RdpAuthMode::NativePrompt, None))
}

fn build_rdp_command_with_prompts<FUser, FPassword>(
    args: &RdpCommandArgs,
    explicit_pass_entry: Option<&str>,
    terminal_prompting_enabled: bool,
    mut prompt_user: FUser,
    mut prompt_password: FPassword,
) -> io::Result<PreparedRdpLaunch>
where
    FUser: FnMut(&InventoryHost) -> io::Result<String>,
    FPassword: FnMut(&InventoryHost) -> io::Result<SensitiveString>,
{
    let auth_settings = crate::config::auth_settings();
    let mut host = configured_rdp_host_for_args(args);

    if host.user.as_deref().filter(|value| !value.trim().is_empty()).is_none() {
        if !terminal_prompting_enabled {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RDP username is required; set `user` in the inventory or pass `--user`",
            ));
        }

        host.user = Some(prompt_user(&host)?);
    }

    let (auth_mode, fallback_notice) =
        resolve_direct_rdp_auth_mode_with(&host, explicit_pass_entry, &auth_settings, terminal_prompting_enabled, &mut prompt_password)?;
    build_prepared_rdp_command(&host, auth_mode, fallback_notice)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_rdp_command_for_host(host: &InventoryHost, explicit_pass_entry: Option<&str>) -> io::Result<PreparedRdpLaunch> {
    let auth_settings = crate::config::auth_settings();
    build_rdp_command_for_host_with_auth_settings(host, explicit_pass_entry, &auth_settings)
}

pub(crate) fn build_rdp_command_for_host_with_auth_settings(
    host: &InventoryHost,
    explicit_pass_entry: Option<&str>,
    auth_settings: &AuthSettings,
) -> io::Result<PreparedRdpLaunch> {
    let unlock_policy = UnlockPolicy::new(auth_settings.idle_timeout_seconds, auth_settings.session_timeout_seconds);
    let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(host, explicit_pass_entry, |pass_entry_name| {
        resolve_vault_password_with_policy(pass_entry_name, unlock_policy.clone()).map_err(|err| err.to_string())
    });
    build_prepared_rdp_command(host, auth_mode, fallback_notice)
}

pub(crate) fn build_rdp_command_for_host_with_manual_password(host: &InventoryHost, password: SensitiveString) -> io::Result<PreparedRdpLaunch> {
    build_prepared_rdp_command(
        host,
        RdpAuthMode::SuppliedPassword {
            password,
            source: RdpCredentialSource::ManualPrompt,
        },
        None,
    )
}

pub(crate) fn build_rdp_command(args: &RdpCommandArgs, explicit_pass_entry: Option<&str>) -> io::Result<PreparedRdpLaunch> {
    build_rdp_command_with_prompts(
        args,
        explicit_pass_entry,
        terminal_prompting_available(),
        prompt_rdp_username,
        prompt_rdp_password,
    )
}

#[cfg(test)]
#[path = "../test/process/rdp_builder.rs"]
mod tests;
