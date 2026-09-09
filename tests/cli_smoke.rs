use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_SERIAL: AtomicU64 = AtomicU64::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new(prefix: &str) -> Self {
        let serial = TEMP_DIR_SERIAL.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("cossh_cli_{prefix}_{}_{serial}", std::process::id()));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_cossh(args: &[&str]) -> Output {
    run_cossh_with_env(args, &[])
}

fn run_cossh_with_env(args: &[&str], env_overrides: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cossh"));
    command.args(args).env("NO_COLOR", "1");
    for (key, value) in env_overrides {
        command.env(key, value);
    }
    command.output().expect("run cossh binary")
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

#[test]
fn rdp_missing_freerdp_dependency_surfaces_actionable_error() {
    let output = run_cossh_with_env(&["rdp", "desktop01", "--user", "alice"], &[("PATH", "/tmp")]);
    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr output");
    assert!(stderr.contains("xfreerdp3/xfreerdp"));
    assert!(stderr.contains("xfreerdp3"));
    assert!(stderr.contains("xfreerdp"));
    assert!(stderr.contains("Install FreeRDP"));
}

#[test]
fn redirected_ssh_command_stdout_contains_only_remote_output() {
    let root = TestDir::new("redirected_ssh_stdout");
    let bin_dir = root.0.join("bin");
    let home_dir = root.0.join("home");
    fs::create_dir(&bin_dir).expect("create mock bin directory");
    fs::create_dir(&home_dir).expect("create mock home directory");

    let ssh_path = bin_dir.join("ssh");
    fs::write(&ssh_path, "#!/bin/sh\nprintf '\\001\\002remote-output\\377'").expect("write mock ssh");
    fs::set_permissions(&ssh_path, fs::Permissions::from_mode(0o755)).expect("make mock ssh executable");

    let output = run_cossh_with_env(
        &["ssh", "example.com", "command"],
        &[
            ("HOME", home_dir.to_str().expect("utf8 home path")),
            ("PATH", bin_dir.to_str().expect("utf8 bin path")),
        ],
    );

    assert!(output.status.success(), "cossh stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"\x01\x02remote-output\xff");
}
