use super::{extract_ssh_destination, resolve_logging_settings};
use cossh::args::MainArgs;
use cossh::log::DebugVerbosity;

fn base_args(debug_count: u8, ssh_logging: bool, test_mode: bool) -> MainArgs {
    MainArgs {
        debug_count,
        ssh_logging,
        test_mode,
        ssh_args: vec!["localhost".to_string()],
        profile: None,
        is_non_interactive: false,
        interactive: false,
        vault_command: None,
        pass_entry: None,
        agent_serve: false,
    }
}

#[test]
fn extracts_plain_destination_host() {
    let args = vec!["example.com".to_string()];
    assert_eq!(extract_ssh_destination(&args).as_deref(), Some("example.com"));
}

#[test]
fn extracts_host_from_user_at_host_syntax() {
    let args = vec!["alice@example.com".to_string()];
    assert_eq!(extract_ssh_destination(&args).as_deref(), Some("example.com"));
}

#[test]
fn skips_flags_that_consume_values_before_destination() {
    let args = vec![
        "-p".to_string(),
        "2222".to_string(),
        "-J".to_string(),
        "jump.example.com".to_string(),
        "target.example.com".to_string(),
    ];
    assert_eq!(extract_ssh_destination(&args).as_deref(), Some("target.example.com"));
}

#[test]
fn returns_none_when_only_value_consuming_flags_are_present() {
    let args = vec!["-W".to_string(), "localhost:22".to_string()];
    assert_eq!(extract_ssh_destination(&args), None);
}

#[test]
fn test_mode_uses_only_cli_logging_flags() {
    let args = base_args(0, false, true);
    assert_eq!(resolve_logging_settings(&args, true, true), (DebugVerbosity::Off, false));

    let args = base_args(1, false, true);
    assert_eq!(resolve_logging_settings(&args, false, true), (DebugVerbosity::Safe, false));

    let args = base_args(2, true, true);
    assert_eq!(resolve_logging_settings(&args, true, false), (DebugVerbosity::Raw, true));
}

#[test]
fn normal_mode_merges_cli_and_config_logging_flags() {
    let args = base_args(0, false, false);
    assert_eq!(resolve_logging_settings(&args, true, true), (DebugVerbosity::Safe, true));

    let args = base_args(1, false, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Safe, false));

    let args = base_args(2, true, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Raw, true));

    let args = base_args(0, false, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Off, false));
}
