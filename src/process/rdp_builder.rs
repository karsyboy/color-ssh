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

fn validate_rdp_value(label: &str, value: &str) -> io::Result<()> {
    if value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("RDP {label} cannot contain carriage returns, newlines, or NUL characters"),
        ));
    }

    Ok(())
}

fn validate_rdp_password_for_startup(password: &SensitiveString) -> io::Result<()> {
    validate_rdp_value("password", password.expose_secret())
}

fn validated_rdp_extra_args(host: &InventoryHost) -> io::Result<Vec<String>> {
    let mut extra_args = Vec::with_capacity(host.rdp.args.len());
    for arg in &host.rdp.args {
        if arg.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "RDP extra arguments cannot be empty"));
        }
        validate_rdp_value("extra argument", arg)?;
        extra_args.push(arg.clone());
    }

    Ok(extra_args)
}

fn rdp_server_address(host: &InventoryHost) -> io::Result<String> {
    let destination = host.host.trim();
    if destination.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP host destination is required; set `host` in the inventory or pass a target",
        ));
    }
    validate_rdp_value("host", destination)?;

    Ok(match host.port {
        Some(port) if destination.contains(':') && !destination.starts_with('[') => format!("[{destination}]:{port}"),
        Some(port) => format!("{destination}:{port}"),
        None => destination.to_string(),
    })
}

fn direct_rdp_vault_autologin_failure(
    detail: impl Into<String>,
    explicit_pass_entry: bool,
    terminal_prompting_enabled: bool,
) -> io::Result<(RdpAuthMode, Option<String>)> {
    let detail = detail.into();

    if explicit_pass_entry {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Password auto-login failed for explicit --pass-entry because {detail}."),
        ));
    }

    if !terminal_prompting_enabled {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Password auto-login is unavailable because {detail}, and no interactive terminal is available for the FreeRDP password prompt."),
        ));
    }

    Ok((RdpAuthMode::NativePrompt, Some(rdp_prompt_fallback_notice(detail))))
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
    let Some(user) = host.user.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP username is required; set `user` in the inventory or pass `--user`",
        ));
    };
    validate_rdp_value("username", user)?;

    let server = rdp_server_address(host)?;
    let extra_args = validated_rdp_extra_args(host)?;
    let mut args = Vec::with_capacity(host.rdp.args.len() + 6);
    args.push(format!("/u:{user}"));
    if let Some(domain) = host.rdp.domain.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        validate_rdp_value("domain", domain)?;
        args.push(format!("/d:{domain}"));
    }
    args.push(format!("/v:{server}"));
    if let Some(password) = password {
        validate_rdp_password_for_startup(password)?;
        args.push(format!("/p:{}", password.expose_secret()));
    } else {
        // Force a terminal-backed credential prompt when vault auto-login is unavailable.
        args.push("+force-console-callbacks".to_string());
        args.push("/from-stdin:force".to_string());
    }
    // Keep default cert handling explicit when caller did not specify one.
    if !has_rdp_cert_flag(&extra_args) {
        args.push("/cert:tofu".to_string());
    }
    args.extend(extra_args);

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

fn configured_rdp_host_for_args(args: &RdpCommandArgs) -> InventoryHost {
    let configured_host = crate::inventory::load_inventory_tree().ok().and_then(|tree| {
        resolve_host_by_destination(&args.target, &tree.hosts)
            .filter(|host| matches!(host.protocol, ConnectionProtocol::Rdp))
            .cloned()
    });

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
        Ok(password) => match validate_rdp_password_for_startup(&password) {
            Ok(()) => (
                RdpAuthMode::SuppliedPassword {
                    password,
                    source: RdpCredentialSource::VaultEntry,
                },
                None,
            ),
            Err(err) => (RdpAuthMode::NativePrompt, Some(rdp_prompt_fallback_notice(err.to_string()))),
        },
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

fn resolve_direct_rdp_auth_mode_with(
    host: &InventoryHost,
    explicit_pass_entry: Option<&str>,
    auth_settings: &AuthSettings,
    terminal_prompting_enabled: bool,
) -> io::Result<(RdpAuthMode, Option<String>)> {
    if !auth_settings.direct_password_autologin {
        return Ok((RdpAuthMode::NativePrompt, None));
    }

    let Some(pass_entry_name) = explicit_pass_entry.map(str::to_string).or_else(|| host.vault_pass.clone()) else {
        return Ok((RdpAuthMode::NativePrompt, None));
    };

    let unlock_policy = UnlockPolicy::new(auth_settings.idle_timeout_seconds, auth_settings.session_timeout_seconds);
    match resolve_vault_password_with_policy(&pass_entry_name, unlock_policy) {
        Ok(password) => {
            if let Err(err) = validate_rdp_password_for_startup(&password) {
                return direct_rdp_vault_autologin_failure(err.to_string(), explicit_pass_entry.is_some(), terminal_prompting_enabled);
            }

            Ok((
                RdpAuthMode::SuppliedPassword {
                    password,
                    source: RdpCredentialSource::VaultEntry,
                },
                None,
            ))
        }
        Err(err) => direct_rdp_vault_autologin_failure(err.to_string(), explicit_pass_entry.is_some(), terminal_prompting_enabled),
    }
}

fn build_rdp_command_with_prompts<FUser>(
    args: &RdpCommandArgs,
    explicit_pass_entry: Option<&str>,
    terminal_prompting_enabled: bool,
    mut prompt_user: FUser,
) -> io::Result<PreparedRdpLaunch>
where
    FUser: FnMut(&InventoryHost) -> io::Result<String>,
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

    let (auth_mode, fallback_notice) = resolve_direct_rdp_auth_mode_with(&host, explicit_pass_entry, &auth_settings, terminal_prompting_enabled)?;
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
    build_rdp_command_with_prompts(args, explicit_pass_entry, terminal_prompting_available(), prompt_rdp_username)
}

#[cfg(test)]
#[path = "../test/process/rdp_builder.rs"]
mod tests;
