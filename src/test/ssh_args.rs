use crate::ssh_args::{extract_destination_host, is_non_interactive_ssh_invocation};

#[test]
fn extract_destination_host_core_forms() {
    assert_eq!(extract_destination_host(&["example.com".to_string()]), Some("example.com".to_string()));
    assert_eq!(extract_destination_host(&["alice@example.com".to_string()]), Some("example.com".to_string()));
    assert_eq!(extract_destination_host(&["-W".to_string(), "localhost:22".to_string()]), None);
}

#[test]
fn non_interactive_detection_passthrough_flags() {
    for flag in ["-G", "-V", "-Q", "-O"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(is_non_interactive_ssh_invocation(&ssh_args));
    }
}

#[test]
fn non_interactive_detection_connection_flags_remain_false() {
    for flag in ["-T", "-N", "-n", "-f", "-W"] {
        let ssh_args = vec![flag.to_string(), "example.com".to_string()];
        assert!(!is_non_interactive_ssh_invocation(&ssh_args));
    }
}
