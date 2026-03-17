use super::{
    protocol_command_for_non_interactive, protocol_reload_notice_target, resolve_runtime_profile_for_command, should_print_title_banner_before_protocol_launch,
};
use crate::args::{MainArgs, MainCommand, ProtocolCommand, RdpCommandArgs, SshCommandArgs, VaultCommand};
use crate::config::ReloadNoticeTarget;
use crate::inventory::{ConnectionProtocol, InventoryHost};

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
fn resolve_runtime_profile_for_command_prefers_explicit_cli_profile() {
    let mut host = InventoryHost::new("router".to_string());
    host.profile = Some("inventory".to_string());

    let profile = resolve_runtime_profile_for_command(Some("cli"), base_args(0, false, false).command.as_ref(), &[host]);

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

    let profile = resolve_runtime_profile_for_command(None, Some(&command), &[host]);

    assert_eq!(profile.as_deref(), Some("network"));
}

#[test]
fn resolve_runtime_profile_for_command_uses_inventory_profile_for_direct_rdp_host() {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.profile = Some("windows".to_string());

    let command = MainCommand::Protocol(ProtocolCommand::Rdp(RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    }));

    let profile = resolve_runtime_profile_for_command(None, Some(&command), &[host]);

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

    let profile = resolve_runtime_profile_for_command(None, Some(&command), &[host]);

    assert_eq!(profile, None);
}

#[test]
fn protocol_reload_notice_target_queues_direct_pty_ssh_notices() {
    let command = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: false,
    });

    assert_eq!(protocol_reload_notice_target(&command, true), ReloadNoticeTarget::Queue);
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
    let rdp = ProtocolCommand::Rdp(RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    });

    assert_eq!(protocol_reload_notice_target(&interactive_ssh, false), ReloadNoticeTarget::Stderr);
    assert_eq!(protocol_reload_notice_target(&non_interactive_ssh, true), ReloadNoticeTarget::Stderr);
    assert_eq!(protocol_reload_notice_target(&rdp, true), ReloadNoticeTarget::Stderr);
}

#[test]
fn title_banner_printing_moves_inside_direct_pty_viewport_for_interactive_ssh() {
    let interactive_ssh = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: false,
    });
    let non_interactive_ssh = ProtocolCommand::Ssh(SshCommandArgs {
        ssh_args: vec!["router01".to_string()],
        is_non_interactive: true,
    });
    let rdp = ProtocolCommand::Rdp(RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    });

    assert!(!should_print_title_banner_before_protocol_launch(&interactive_ssh, true));
    assert!(should_print_title_banner_before_protocol_launch(&interactive_ssh, false));
    assert!(should_print_title_banner_before_protocol_launch(&non_interactive_ssh, true));
    assert!(should_print_title_banner_before_protocol_launch(&rdp, true));
}

#[test]
fn protocol_command_for_non_interactive_requires_protocol_command() {
    let mut no_command = base_args(0, false, false);
    no_command.command = None;
    assert!(protocol_command_for_non_interactive(&no_command).is_err());

    let mut vault_command = base_args(0, false, false);
    vault_command.command = Some(MainCommand::Vault(VaultCommand::Status));
    assert!(protocol_command_for_non_interactive(&vault_command).is_err());
}
