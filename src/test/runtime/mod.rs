use super::{DebugModeSource, debug_mode_source, resolve_logging_settings, resolve_session_name_for_logging};
use crate::args::{MainArgs, MainCommand, ProtocolCommand, SshCommandArgs};
use crate::config::ReloadNoticeTarget;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::log::DebugVerbosity;

fn base_args(debug_count: u8, ssh_logging: bool, test_mode: bool) -> MainArgs {
    MainArgs {
        debug_count,
        ssh_logging,
        test_mode,
        profile: None,
        interactive: false,
        pass_entry: None,
        command: Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["localhost".to_string()],
            is_non_interactive: false,
        }))),
    }
}

#[test]
fn resolve_logging_settings_matches_safe_and_raw_modes() {
    assert_eq!(resolve_logging_settings(&base_args(1, false, true), false, true), (DebugVerbosity::Safe, false));
    assert_eq!(resolve_logging_settings(&base_args(2, true, false), false, false), (DebugVerbosity::Raw, true));
}

#[test]
fn debug_mode_source_prefers_cli_then_config() {
    assert_eq!(debug_mode_source(&base_args(2, false, false), true), Some(DebugModeSource::CliRaw));
    assert_eq!(debug_mode_source(&base_args(0, false, false), true), Some(DebugModeSource::ConfigSafe));
}

#[test]
fn resolve_session_name_for_logging_prefers_target_then_ssh_args_and_sanitizes() {
    assert_eq!(resolve_session_name_for_logging(Some("desktop01"), &[]), "desktop01");
    assert_eq!(resolve_session_name_for_logging(Some("bad/name"), &[]), "bad_name");
    assert_eq!(resolve_session_name_for_logging(None, &["admin@router01".to_string()]), "router01");
    assert_eq!(resolve_session_name_for_logging(None, &[]), "unknown");
}

#[test]
fn resolve_runtime_profile_for_command_prefers_explicit_cli_profile() {
    let mut host = InventoryHost::new("router".to_string());
    host.profile = Some("inventory".to_string());

    let profile = super::dispatch::resolve_runtime_profile_for_command(Some("cli"), base_args(0, false, false).command.as_ref(), &[host]);

    assert_eq!(profile.as_deref(), Some("cli"));
}

#[test]
fn resolve_runtime_profile_for_command_uses_inventory_profile_for_direct_ssh_host() {
    let mut host = InventoryHost::new("router".to_string());
    host.host = "10.0.0.10".to_string();
    host.profile = Some("network".to_string());

    let command = MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["admin@router".to_string()],
        is_non_interactive: false,
    }));

    let profile = super::dispatch::resolve_runtime_profile_for_command(None, Some(&command), &[host]);

    assert_eq!(profile.as_deref(), Some("network"));
}

#[test]
fn resolve_runtime_profile_for_command_uses_inventory_profile_for_direct_rdp_host() {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.profile = Some("windows".to_string());

    let command = MainCommand::Protocol(ProtocolCommand::Rdp(crate::args::RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    }));

    let profile = super::dispatch::resolve_runtime_profile_for_command(None, Some(&command), &[host]);

    assert_eq!(profile.as_deref(), Some("windows"));
}

#[test]
fn resolve_runtime_profile_for_command_ignores_inventory_hosts_with_wrong_protocol() {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.profile = Some("windows".to_string());

    let command = MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["desktop01".to_string()],
        is_non_interactive: false,
    }));

    let profile = super::dispatch::resolve_runtime_profile_for_command(None, Some(&command), &[host]);

    assert_eq!(profile, None);
}

#[test]
fn protocol_reload_notice_target_queues_direct_pty_ssh_notices() {
    let command = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: false,
    });

    assert_eq!(super::dispatch::protocol_reload_notice_target(&command, true), ReloadNoticeTarget::Queue);
}

#[test]
fn protocol_reload_notice_target_keeps_passthrough_paths_on_stderr() {
    let interactive_ssh = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: false,
    });
    let non_interactive_ssh = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: true,
    });
    let rdp = ProtocolCommand::Rdp(crate::args::RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    });

    assert_eq!(
        super::dispatch::protocol_reload_notice_target(&interactive_ssh, false),
        ReloadNoticeTarget::Stderr
    );
    assert_eq!(
        super::dispatch::protocol_reload_notice_target(&non_interactive_ssh, true),
        ReloadNoticeTarget::Stderr
    );
    assert_eq!(super::dispatch::protocol_reload_notice_target(&rdp, true), ReloadNoticeTarget::Stderr);
}

#[test]
fn protocol_command_for_non_interactive_requires_protocol_command() {
    let mut no_command = base_args(0, false, false);
    no_command.command = None;
    assert!(super::dispatch::protocol_command_for_non_interactive(&no_command).is_err());

    let mut vault_command = base_args(0, false, false);
    vault_command.command = Some(MainCommand::Vault(crate::args::VaultCommand::Status));
    assert!(super::dispatch::protocol_command_for_non_interactive(&vault_command).is_err());
}
