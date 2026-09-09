use super::{build_ssh_command_for_host, synthesize_ssh_args};
use crate::inventory::InventoryHost;
use std::process::Command;

fn assert_pair(args: &[String], flag: &str, value: &str) {
    assert!(
        args.windows(2).any(|window| window[0] == flag && window[1] == value),
        "missing pair [{flag}, {value}] in args: {args:?}"
    );
}

fn assert_contains(args: &[String], expected: &str) {
    assert!(args.iter().any(|arg| arg == expected), "missing arg '{expected}' in args: {args:?}");
}

fn ssh_effective_value(args: &[String], key: &str) -> String {
    let output = Command::new("ssh").args(["-G", "-F", "/dev/null"]).args(args).output().expect("run ssh -G");
    assert!(output.status.success(), "ssh -G failed: {}", String::from_utf8_lossy(&output.stderr));

    String::from_utf8(output.stdout)
        .expect("ssh -G output is UTF-8")
        .lines()
        .find_map(|line| line.split_once(' ').filter(|(name, _)| name.eq_ignore_ascii_case(key)))
        .map(|(_, value)| value.to_string())
        .unwrap_or_else(|| panic!("ssh -G did not emit {key}"))
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
fn synthesize_ssh_args_honors_post_destination_port_override() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.port = Some(2222);

    let effective_args = synthesize_ssh_args(&["switch".to_string(), "-p".to_string(), "2200".to_string()], &host);

    assert_eq!(effective_args, ["10.0.0.10", "-p", "2200"]);
}

#[test]
fn synthesize_ssh_args_resolves_destination_after_bundled_value_flag() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.port = Some(2222);

    let effective_args = synthesize_ssh_args(&["-vp".to_string(), "2200".to_string(), "switch".to_string()], &host);

    assert_eq!(effective_args, ["-vp", "2200", "10.0.0.10"]);
}

#[test]
fn synthesize_ssh_args_preserves_whitespace_option_overrides_and_option_terminator() {
    let mut host = InventoryHost::new("alpha".to_string());
    host.host = "example.com".to_string();
    host.port = Some(2222);

    let override_args = synthesize_ssh_args(&["alpha".to_string(), "-o".to_string(), "Port 2200".to_string()], &host);
    assert_eq!(override_args, ["example.com", "-o", "Port 2200"]);
    assert_eq!(ssh_effective_value(&override_args, "port"), "2200");

    let terminated_args = synthesize_ssh_args(&["-v".to_string(), "--".to_string(), "alpha".to_string()], &host);
    assert_eq!(terminated_args, ["-v", "-p", "2222", "--", "example.com"]);
    assert_eq!(ssh_effective_value(&terminated_args, "port"), "2222");
    assert_eq!(ssh_effective_value(&terminated_args, "hostname"), "example.com");
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
