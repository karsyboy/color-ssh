use super::{MainCommand, ProtocolCommand, RdpCommandArgs, SshCommandArgs, VaultCommand, build_cli_command, parse_main_args_from};
use crate::ssh_args::is_non_interactive_ssh_invocation;

#[test]
fn enters_interactive_mode_with_no_user_args() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh"]);
    assert!(parsed.interactive);
    assert!(parsed.command.is_none());
}

#[test]
fn enters_interactive_mode_for_debug_only() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-d"]);
    assert!(parsed.interactive);
    assert_eq!(parsed.debug_count, 1);
    assert!(parsed.command.is_none());
}

#[test]
fn parses_ssh_subcommand_direct_launch() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-d", "ssh", "user@example.com"]);

    assert!(!parsed.interactive);
    assert_eq!(parsed.debug_count, 1);
    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["user@example.com".to_string()],
            is_non_interactive: false,
        })))
    );
}

#[test]
fn detects_non_interactive_passthrough_flags() {
    for flag in ["-G", "-V", "-Q", "-O"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(is_non_interactive_ssh_invocation(&ssh_args), "flag {flag} should be passthrough");
    }
}

#[test]
fn does_not_detect_connection_mode_flags_as_passthrough() {
    for flag in ["-T", "-N", "-n", "-f", "-W"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(
            !is_non_interactive_ssh_invocation(&ssh_args),
            "flag {flag} should stay in normal connection pipeline"
        );
    }
    let ssh_args = vec!["user@example.com".to_string()];
    assert!(!is_non_interactive_ssh_invocation(&ssh_args));
}

#[test]
fn parses_test_mode_and_combined_short_flags() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-tld", "ssh", "localhost"]);

    assert!(parsed.test_mode);
    assert_eq!(parsed.debug_count, 1);
    assert!(parsed.ssh_logging);
    assert!(!parsed.interactive);
    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["localhost".to_string()],
            is_non_interactive: false,
        })))
    );
}

#[test]
fn parses_vault_add_pass_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "vault", "add", "office_fw"]);

    assert_eq!(parsed.command, Some(MainCommand::Vault(VaultCommand::AddPass("office_fw".to_string()))));
    assert!(!parsed.interactive);
}

#[test]
fn parses_vault_add_pass_with_debug() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--debug", "vault", "add", "office_fw"]);

    assert_eq!(parsed.debug_count, 1);
    assert_eq!(parsed.command, Some(MainCommand::Vault(VaultCommand::AddPass("office_fw".to_string()))));
}

#[test]
fn parses_repeated_debug_flags_into_raw_debug_mode() {
    let cmd = build_cli_command();

    let parsed = parse_main_args_from(&cmd, ["cossh", "-dd", "ssh", "user@example.com"]);
    assert_eq!(parsed.debug_count, 2);

    let parsed = parse_main_args_from(&cmd, ["cossh", "--debug", "--debug", "ssh", "user@example.com"]);
    assert_eq!(parsed.debug_count, 2);

    let parsed = parse_main_args_from(&cmd, ["cossh", "-ddd", "ssh", "user@example.com"]);
    assert_eq!(parsed.debug_count, 3);
}

#[test]
fn parses_vault_list_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "vault", "list"]);

    assert_eq!(parsed.command, Some(MainCommand::Vault(VaultCommand::List)));
    assert!(!parsed.interactive);
}

#[test]
fn parses_vault_init_unlock_and_status_modes() {
    let cmd = build_cli_command();

    let init = parse_main_args_from(&cmd, ["cossh", "vault", "init"]);
    assert_eq!(init.command, Some(MainCommand::Vault(VaultCommand::Init)));

    let unlock = parse_main_args_from(&cmd, ["cossh", "vault", "unlock"]);
    assert_eq!(unlock.command, Some(MainCommand::Vault(VaultCommand::Unlock)));

    let status = parse_main_args_from(&cmd, ["cossh", "vault", "status"]);
    assert_eq!(status.command, Some(MainCommand::Vault(VaultCommand::Status)));
}

#[test]
fn parses_pass_entry_override_with_direct_launch() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--pass-entry", "office_fw", "ssh", "user@example.com"]);

    assert_eq!(parsed.pass_entry.as_deref(), Some("office_fw"));
    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["user@example.com".to_string()],
            is_non_interactive: false,
        })))
    );
}

#[test]
fn parses_hidden_agent_serve_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "agent", "--serve"]);

    assert_eq!(parsed.command, Some(MainCommand::AgentServe));
    assert!(!parsed.interactive);
}

#[test]
fn parses_rdp_subcommand_with_overrides_and_extra_args() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(
        &cmd,
        [
            "cossh",
            "--pass-entry",
            "office_rdp",
            "rdp",
            "desktop01",
            "--user",
            "administrator",
            "--domain",
            "ACME",
            "--port",
            "3390",
            "/f",
            "+clipboard",
        ],
    );

    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Rdp(RdpCommandArgs {
            target: "desktop01".to_string(),
            user: Some("administrator".to_string()),
            domain: Some("ACME".to_string()),
            port: Some(3390),
            extra_args: vec!["/f".to_string(), "+clipboard".to_string()],
        })))
    );
    assert_eq!(parsed.pass_entry.as_deref(), Some("office_rdp"));
    assert!(!parsed.interactive);
}

#[test]
fn rejects_vault_add_pass_with_ssh_args() {
    let cmd = build_cli_command();
    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "vault", "add", "office_fw", "user@example.com"])
            .is_err()
    );
}

#[test]
fn parses_ssh_non_interactive_passthrough_forms() {
    let cmd = build_cli_command();

    let parsed = parse_main_args_from(&cmd, ["cossh", "ssh", "user@example.com", "-G"]);
    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["user@example.com".to_string(), "-G".to_string()],
            is_non_interactive: true,
        })))
    );

    let parsed = parse_main_args_from(&cmd, ["cossh", "ssh", "-G", "user@example.com"]);
    assert_eq!(
        parsed.command,
        Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["-G".to_string(), "user@example.com".to_string()],
            is_non_interactive: true,
        })))
    );
}

#[test]
fn rejects_vault_subcommand_with_profile_log_and_test_flags_after_it() {
    let cmd = build_cli_command();

    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "vault", "add", "office_fw", "--profile", "network"])
            .is_err()
    );
    assert!(cmd.clone().try_get_matches_from(["cossh", "vault", "add", "office_fw", "--log"]).is_err());
    assert!(cmd.try_get_matches_from(["cossh", "vault", "add", "office_fw", "--test"]).is_err());
}

#[test]
fn rejects_invalid_profile_names() {
    let cmd = build_cli_command();

    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "--profile", "../prod", "ssh", "user@example.com"])
            .is_err()
    );
    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "--profile", "prod/main", "ssh", "user@example.com"])
            .is_err()
    );
    assert!(
        cmd.try_get_matches_from(["cossh", "--profile", "prod.config", "ssh", "user@example.com"])
            .is_err()
    );
}

#[test]
fn rejects_legacy_implicit_ssh_syntax() {
    let cmd = build_cli_command();
    assert!(cmd.try_get_matches_from(["cossh", "user@example.com"]).is_err());
}

#[test]
fn rejects_ssh_subcommand_without_forwarded_args() {
    let cmd = build_cli_command();
    assert!(cmd.try_get_matches_from(["cossh", "ssh"]).is_err());
}
