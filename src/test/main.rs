use super::{
    extract_ssh_destination, pass_key_for_destination_from_hosts, resolve_logging_settings, skip_pass_resolution_for_context, skip_pass_resolution_from_env,
};
use cossh::args::MainArgs;
use cossh::ssh_config::SshHost;

fn base_args(debug: bool, ssh_logging: bool, test_mode: bool) -> MainArgs {
    MainArgs {
        debug,
        ssh_logging,
        test_mode,
        ssh_args: vec!["localhost".to_string()],
        profile: None,
        is_non_interactive: false,
        interactive: false,
        add_pass: None,
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
    let args = base_args(false, false, true);
    assert_eq!(resolve_logging_settings(&args, true, true), (false, false));

    let args = base_args(true, false, true);
    assert_eq!(resolve_logging_settings(&args, false, true), (true, false));

    let args = base_args(false, true, true);
    assert_eq!(resolve_logging_settings(&args, true, false), (false, true));
}

#[test]
fn normal_mode_merges_cli_and_config_logging_flags() {
    let args = base_args(false, false, false);
    assert_eq!(resolve_logging_settings(&args, true, true), (true, true));

    let args = base_args(true, false, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (true, false));

    let args = base_args(false, true, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (false, true));
}

#[test]
fn pass_key_lookup_matches_exact_alias_only() {
    let mut exact = SshHost::new("target".to_string());
    exact.pass_key = Some("shared".to_string());

    let mut other = SshHost::new("target-prod".to_string());
    other.pass_key = Some("other".to_string());

    let hosts = vec![other, exact];
    assert_eq!(pass_key_for_destination_from_hosts("target", &hosts).as_deref(), Some("shared"));
    assert_eq!(pass_key_for_destination_from_hosts("tar", &hosts), None);
}

#[test]
fn skip_pass_resolution_env_parser_accepts_booleanish_values() {
    assert!(skip_pass_resolution_from_env(Some("1")));
    assert!(skip_pass_resolution_from_env(Some("true")));
    assert!(skip_pass_resolution_from_env(Some("YES")));
    assert!(!skip_pass_resolution_from_env(Some("0")));
    assert!(!skip_pass_resolution_from_env(Some("")));
    assert!(!skip_pass_resolution_from_env(None));
}

#[test]
fn skip_pass_resolution_requires_tui_session_context() {
    assert!(skip_pass_resolution_for_context(Some("1"), Some("tab-1")));
    assert!(!skip_pass_resolution_for_context(Some("1"), None));
    assert!(!skip_pass_resolution_for_context(Some("1"), Some("   ")));
    assert!(!skip_pass_resolution_for_context(Some("0"), Some("tab-1")));
}
