use super::{VaultCommand, build_cli_command, detect_non_interactive_ssh_args, parse_main_args_from};

#[test]
fn enters_interactive_mode_with_no_user_args() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh"]);
    assert!(parsed.interactive);
    assert!(parsed.ssh_args.is_empty());
}

#[test]
fn enters_interactive_mode_for_debug_only() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-d"]);
    assert!(parsed.interactive);
    assert_eq!(parsed.debug_count, 1);
    assert!(parsed.ssh_args.is_empty());
}

#[test]
fn does_not_enter_interactive_mode_when_connect_target_is_present() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-d", "user@example.com"]);
    assert!(!parsed.interactive);
    assert_eq!(parsed.ssh_args, vec!["user@example.com".to_string()]);
}

#[test]
fn detects_non_interactive_passthrough_flags() {
    for flag in ["-G", "-V", "-Q", "-O"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(detect_non_interactive_ssh_args(&ssh_args), "flag {flag} should be passthrough");
    }
}

#[test]
fn does_not_detect_connection_mode_flags_as_passthrough() {
    for flag in ["-T", "-N", "-n", "-f", "-W"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(
            !detect_non_interactive_ssh_args(&ssh_args),
            "flag {flag} should stay in normal connection pipeline"
        );
    }
    let ssh_args = vec!["user@example.com".to_string()];
    assert!(!detect_non_interactive_ssh_args(&ssh_args));
}

#[test]
fn parses_test_mode_and_combined_short_flags() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "-tld", "localhost"]);

    assert!(parsed.test_mode);
    assert_eq!(parsed.debug_count, 1);
    assert!(parsed.ssh_logging);
    assert!(!parsed.interactive);
    assert_eq!(parsed.ssh_args, vec!["localhost".to_string()]);
}

#[test]
fn parses_vault_add_pass_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "vault", "add", "office_fw"]);

    assert_eq!(parsed.vault_command, Some(VaultCommand::AddPass("office_fw".to_string())));
    assert!(!parsed.interactive);
    assert!(parsed.ssh_args.is_empty());
}

#[test]
fn parses_vault_add_pass_with_debug() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--debug", "vault", "add", "office_fw"]);

    assert_eq!(parsed.debug_count, 1);
    assert_eq!(parsed.vault_command, Some(VaultCommand::AddPass("office_fw".to_string())));
}

#[test]
fn parses_repeated_debug_flags_into_raw_debug_mode() {
    let cmd = build_cli_command();

    let parsed = parse_main_args_from(&cmd, ["cossh", "-dd", "user@example.com"]);
    assert_eq!(parsed.debug_count, 2);

    let parsed = parse_main_args_from(&cmd, ["cossh", "--debug", "--debug", "user@example.com"]);
    assert_eq!(parsed.debug_count, 2);

    let parsed = parse_main_args_from(&cmd, ["cossh", "-ddd", "user@example.com"]);
    assert_eq!(parsed.debug_count, 3);
}

#[test]
fn parses_vault_list_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "vault", "list"]);

    assert_eq!(parsed.vault_command, Some(VaultCommand::List));
    assert!(!parsed.interactive);
    assert!(parsed.ssh_args.is_empty());
}

#[test]
fn parses_vault_init_unlock_and_status_modes() {
    let cmd = build_cli_command();

    let init = parse_main_args_from(&cmd, ["cossh", "vault", "init"]);
    assert_eq!(init.vault_command, Some(VaultCommand::Init));
    assert!(init.ssh_args.is_empty());

    let unlock = parse_main_args_from(&cmd, ["cossh", "vault", "unlock"]);
    assert_eq!(unlock.vault_command, Some(VaultCommand::Unlock));
    assert!(unlock.ssh_args.is_empty());

    let status = parse_main_args_from(&cmd, ["cossh", "vault", "status"]);
    assert_eq!(status.vault_command, Some(VaultCommand::Status));
    assert!(status.ssh_args.is_empty());
}

#[test]
fn parses_pass_entry_override_with_direct_launch() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--pass-entry", "office_fw", "user@example.com"]);

    assert_eq!(parsed.pass_entry.as_deref(), Some("office_fw"));
    assert_eq!(parsed.ssh_args, vec!["user@example.com".to_string()]);
}

#[test]
fn parses_hidden_agent_serve_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "agent", "--serve"]);

    assert!(parsed.agent_serve);
    assert!(parsed.ssh_args.is_empty());
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

    assert!(cmd.clone().try_get_matches_from(["cossh", "--profile", "../prod", "user@example.com"]).is_err());
    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "--profile", "prod/main", "user@example.com"])
            .is_err()
    );
    assert!(cmd.try_get_matches_from(["cossh", "--profile", "prod.config", "user@example.com"]).is_err());
}
