use super::{build_cli_command, detect_non_interactive_ssh_args, parse_main_args_from};

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
    assert!(parsed.debug);
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
    assert!(parsed.debug);
    assert!(parsed.ssh_logging);
    assert!(!parsed.interactive);
    assert_eq!(parsed.ssh_args, vec!["localhost".to_string()]);
}

#[test]
fn parses_add_pass_mode() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--add-pass", "office_fw"]);

    assert_eq!(parsed.add_pass.as_deref(), Some("office_fw"));
    assert!(!parsed.interactive);
    assert!(parsed.ssh_args.is_empty());
}

#[test]
fn parses_add_pass_with_debug() {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, ["cossh", "--debug", "--add-pass", "office_fw"]);

    assert!(parsed.debug);
    assert_eq!(parsed.add_pass.as_deref(), Some("office_fw"));
}

#[test]
fn rejects_add_pass_with_ssh_args() {
    let cmd = build_cli_command();
    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "--add-pass", "office_fw", "user@example.com"])
            .is_err()
    );
}

#[test]
fn rejects_add_pass_with_profile_log_and_test_flags() {
    let cmd = build_cli_command();

    assert!(
        cmd.clone()
            .try_get_matches_from(["cossh", "--add-pass", "office_fw", "--profile", "network"])
            .is_err()
    );
    assert!(cmd.clone().try_get_matches_from(["cossh", "--add-pass", "office_fw", "--log"]).is_err());
    assert!(cmd.try_get_matches_from(["cossh", "--add-pass", "office_fw", "--test"]).is_err());
}
