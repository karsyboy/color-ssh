use std::process::{Command, Output};

fn run_cossh(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cossh"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run cossh binary")
}

#[test]
fn help_lists_primary_command_surface() {
    let output = run_cossh(&["--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("utf8 help output");
    assert!(stdout.contains("Usage: cossh"));
    assert!(stdout.contains("ssh"));
    assert!(stdout.contains("rdp"));
    assert!(stdout.contains("vault"));
}

#[test]
fn version_reports_current_package_version() {
    let output = run_cossh(&["--version"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("utf8 version output");
    assert!(stdout.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn invalid_profile_input_returns_cli_validation_error() {
    let output = run_cossh(&["--profile", "../bad", "ssh", "host"]);
    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr output");
    assert!(stderr.contains("invalid profile name"));
}
