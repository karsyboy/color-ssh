use super::{create_private_directory, open_private_append_file, should_flush};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
fn temp_path(prefix: &str) -> PathBuf {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock should be after epoch").as_nanos();
    std::env::temp_dir().join(format!("cossh-log-debug-{prefix}-{unique}"))
}

#[test]
fn should_flush_on_size_or_interval() {
    assert!(!should_flush(512, Duration::from_millis(20)));
    assert!(should_flush(16 * 1024, Duration::from_millis(20)));
    assert!(should_flush(1, Duration::from_millis(100)));
}

#[cfg(unix)]
#[test]
fn private_directory_and_file_permissions_are_restrictive() {
    let root = temp_path("permissions");
    let log_dir = root.join("logs");
    let log_path = log_dir.join("cossh.log");

    create_private_directory(&log_dir).expect("create private log directory");
    let _file = open_private_append_file(&log_path).expect("create private log file");

    let dir_mode = fs::metadata(&log_dir).expect("directory metadata").permissions().mode() & 0o777;
    let file_mode = fs::metadata(&log_path).expect("file metadata").permissions().mode() & 0o777;

    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);

    let _ = fs::remove_dir_all(root);
}
