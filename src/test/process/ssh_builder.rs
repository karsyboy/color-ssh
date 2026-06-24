use super::{build_ssh_command_for_host, synthesize_ssh_args};
use crate::inventory::InventoryHost;

fn assert_pair(args: &[String], flag: &str, value: &str) {
    assert!(
        args.windows(2).any(|window| window[0] == flag && window[1] == value),
        "missing pair [{flag}, {value}] in args: {args:?}"
    );
}

fn assert_contains(args: &[String], expected: &str) {
    assert!(args.iter().any(|arg| arg == expected), "missing arg '{expected}' in args: {args:?}");
}

#[test]
fn synthesize_ssh_args_inventory_defaults_and_cli_overrides_apply_expected_precedence() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.user = Some("admin".to_string());
    host.port = Some(2222);
    host.ssh.proxy_jump = Some("bastion".to_string());

    let default_args = synthesize_ssh_args(&["switch".to_string()], &host);
    assert_pair(&default_args, "-l", "admin");
    assert_pair(&default_args, "-p", "2222");
    assert_contains(&default_args, "ProxyJump=bastion");

    let cli_args = vec![
        "-l".to_string(),
        "override".to_string(),
        "-o".to_string(),
        "ProxyJump=direct".to_string(),
        "user@switch".to_string(),
    ];
    let overridden_args = synthesize_ssh_args(&cli_args, &host);

    assert_pair(&overridden_args, "-l", "override");
    assert_contains(&overridden_args, "ProxyJump=direct");
    assert_contains(&overridden_args, "user@10.0.0.10");
}

#[test]
fn build_ssh_command_for_host_uses_synthesized_inventory_defaults() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.user = Some("admin".to_string());
    host.port = Some(2222);

    let command = build_ssh_command_for_host(&host, None).expect("build ssh command for host");

    assert_eq!(command.program, "ssh");
    assert_pair(&command.args, "-l", "admin");
    assert_pair(&command.args, "-p", "2222");
    assert_contains(&command.args, "10.0.0.10");
    assert!(command.stdin_payload.is_none());
}
