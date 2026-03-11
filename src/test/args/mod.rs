use super::{
    CompletionProtocol, MainArgs, MainCommand, ProtocolCommand, RdpCommandArgs, SshCommandArgs, VaultCommand, build_cli_command, parse_main_args_from,
    try_parse_main_args_from,
};

fn parse_ok(args: &[&str]) -> MainArgs {
    let command = build_cli_command();
    parse_main_args_from(&command, args.iter().copied())
}

fn assert_parse_err(args: &[&str]) {
    let command = build_cli_command();
    assert!(
        try_parse_main_args_from(&command, args.iter().copied()).is_err(),
        "expected parse error for args: {args:?}"
    );
}

#[test]
fn parse_main_args_default_and_debug_flags_set_interactive_and_debug_levels() {
    let default = parse_ok(&["cossh"]);
    assert!(default.interactive);

    let debug_only = parse_ok(&["cossh", "-d"]);
    assert!(debug_only.interactive);
    assert_eq!(debug_only.debug_count, 1);

    let profile_only = parse_ok(&["cossh", "-P", "network"]);
    assert!(profile_only.interactive);
    assert_eq!(profile_only.profile.as_deref(), Some("network"));

    let debug_with_command = parse_ok(&["cossh", "-ddd", "ssh", "host"]);
    assert_eq!(debug_with_command.debug_count, 3);

    let with_pass_entry = parse_ok(&["cossh", "--pass-entry", "shared", "ssh", "host"]);
    assert_eq!(with_pass_entry.pass_entry.as_deref(), Some("shared"));
}

#[test]
fn parse_main_args_protocol_commands_map_to_expected_command_payloads() {
    let cases: Vec<(Vec<&str>, MainCommand)> = vec![
        (
            vec!["cossh", "ssh", "user@example.com"],
            MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
                ssh_args: vec!["user@example.com".to_string()],
                is_non_interactive: false,
            })),
        ),
        (
            vec!["cossh", "ssh", "-G", "user@example.com"],
            MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
                ssh_args: vec!["-G".to_string(), "user@example.com".to_string()],
                is_non_interactive: true,
            })),
        ),
        (
            vec![
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
            ],
            MainCommand::Protocol(ProtocolCommand::Rdp(RdpCommandArgs {
                target: "desktop01".to_string(),
                user: Some("administrator".to_string()),
                domain: Some("ACME".to_string()),
                port: Some(3390),
                extra_args: vec!["/f".to_string()],
            })),
        ),
    ];

    for (args, expected_command) in cases {
        let parsed = parse_ok(&args);
        assert_eq!(parsed.command, Some(expected_command));
    }
}

#[test]
fn parse_main_args_vault_and_migrate_commands_map_to_expected_variants() {
    let vault_cases: Vec<(Vec<&str>, MainCommand)> = vec![
        (
            vec!["cossh", "vault", "add", "office_fw"],
            MainCommand::Vault(VaultCommand::AddPass("office_fw".to_string())),
        ),
        (vec!["cossh", "vault", "list"], MainCommand::Vault(VaultCommand::List)),
        (vec!["cossh", "vault", "init"], MainCommand::Vault(VaultCommand::Init)),
    ];

    for (args, expected_command) in vault_cases {
        assert_eq!(parse_ok(&args).command, Some(expected_command));
    }

    assert_eq!(parse_ok(&["cossh", "--migrate"]).command, Some(MainCommand::MigrateInventory));
    assert_eq!(
        parse_ok(&["cossh", "__complete", "hosts", "--protocol", "ssh"]).command,
        Some(MainCommand::CompletionHosts(CompletionProtocol::Ssh))
    );
}

#[test]
fn parse_main_args_invalid_combinations_and_profile_names_return_parse_errors() {
    let invalid_cases: Vec<Vec<&str>> = vec![
        vec!["cossh", "vault", "add", "office_fw", "user@example.com"],
        vec!["cossh", "--migrate", "ssh", "host"],
        vec!["cossh", "--migrate", "--profile", "network"],
        vec!["cossh", "user@example.com"],
        vec!["cossh", "ssh"],
        vec!["cossh", "--profile", "../prod", "ssh", "host"],
        vec!["cossh", "--profile", "prod/main", "ssh", "host"],
        vec!["cossh", "--profile", "prod.config", "ssh", "host"],
    ];

    for args in invalid_cases {
        assert_parse_err(&args);
    }
}

#[test]
fn parse_main_args_logging_and_pass_flags_require_protocol_subcommands() {
    let invalid_cases: Vec<Vec<&str>> = vec![
        vec!["cossh", "--log"],
        vec!["cossh", "--test"],
        vec!["cossh", "--pass-entry", "shared"],
        vec!["cossh", "--log", "vault", "status"],
        vec!["cossh", "--test", "vault", "status"],
        vec!["cossh", "--pass-entry", "shared", "vault", "status"],
    ];

    for args in invalid_cases {
        assert_parse_err(&args);
    }
}
