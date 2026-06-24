use super::build_plain_ssh_command;
use crate::auth::secret::SensitiveString;

#[test]
fn build_plain_ssh_command_args_preserves_program_args_and_stdin_payload() {
    let args = vec!["user@host".to_string(), "-p".to_string(), "22".to_string()];

    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.stdin_payload.is_none());
}

#[test]
fn prepared_command_debug_redacts_env_values_and_stdin_payload() {
    let mut command = build_plain_ssh_command(&["user@host".to_string()]);
    command.env.push(("COSSH_INTERNAL_ASKPASS_TOKEN".to_string(), "askpass-secret".to_string()));
    command.stdin_payload = Some(SensitiveString::from("stdin-secret"));

    let debug = format!("{command:?}");

    assert!(debug.contains("COSSH_INTERNAL_ASKPASS_TOKEN"));
    assert!(!debug.contains("askpass-secret"));
    assert!(!debug.contains("stdin-secret"));
    assert!(debug.contains("[REDACTED]"));
}
