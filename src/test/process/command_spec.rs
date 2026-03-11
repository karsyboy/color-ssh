use super::build_plain_ssh_command;

#[test]
fn build_plain_ssh_command_args_preserves_program_args_and_stdin_payload() {
    let args = vec!["user@host".to_string(), "-p".to_string(), "22".to_string()];

    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.stdin_payload.is_none());
}
