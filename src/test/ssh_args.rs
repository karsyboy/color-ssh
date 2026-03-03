use crate::ssh_args::{extract_destination_host, is_non_interactive_ssh_invocation};

#[test]
fn extracts_plain_destination_host() {
    let args = vec!["example.com".to_string()];
    assert_eq!(extract_destination_host(&args).as_deref(), Some("example.com"));
}

#[test]
fn extracts_host_from_user_at_host_syntax() {
    let args = vec!["alice@example.com".to_string()];
    assert_eq!(extract_destination_host(&args).as_deref(), Some("example.com"));
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
    assert_eq!(extract_destination_host(&args).as_deref(), Some("target.example.com"));
}

#[test]
fn returns_none_when_only_value_consuming_flags_are_present() {
    let args = vec!["-W".to_string(), "localhost:22".to_string()];
    assert_eq!(extract_destination_host(&args), None);
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
