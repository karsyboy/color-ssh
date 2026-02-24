use super::{
    LogFileFactory, SshLogCommand, create_private_directory, extract_complete_lines, open_private_append_file, refresh_secret_patterns_if_needed, run_worker,
    sanitize_line, should_flush,
};
use regex::Regex;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn temp_log_path() -> PathBuf {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock should be after epoch").as_nanos();
    std::env::temp_dir().join(format!("cossh-ssh-log-test-{}.log", unique))
}

#[test]
fn extract_complete_lines_keeps_partial_tail() {
    let mut buffer = "one\ntwo\npartial".to_string();
    let lines = extract_complete_lines(&mut buffer);

    assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
    assert_eq!(buffer, "partial");
}

#[test]
fn sanitize_line_strips_ansi_and_redacts_patterns() {
    let secrets = vec![Regex::new("token=\\w+").expect("regex compiles")];
    let line = "\x1b[31mtoken=abc123\x1b[0m ok";
    let sanitized = sanitize_line(line, &secrets);
    assert_eq!(sanitized, "[REDACTED] ok");
}

#[test]
fn should_flush_on_size_or_interval() {
    assert!(!should_flush(1024, Duration::from_millis(20)));
    assert!(should_flush(64 * 1024, Duration::from_millis(20)));
    assert!(should_flush(1, Duration::from_millis(100)));
}

#[cfg(unix)]
#[test]
fn private_directory_and_file_permissions_are_restrictive() {
    let root = std::env::temp_dir().join(format!(
        "cossh-ssh-log-permissions-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).expect("clock should be after epoch").as_nanos()
    ));
    let log_dir = root.join("ssh_sessions");
    let log_path = log_dir.join("session.log");

    create_private_directory(&log_dir).expect("create private ssh log directory");
    let _file = open_private_append_file(&log_path).expect("create private ssh log file");

    let dir_mode = fs::metadata(&log_dir).expect("directory metadata").permissions().mode() & 0o777;
    let file_mode = fs::metadata(&log_path).expect("file metadata").permissions().mode() & 0o777;

    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn refresh_secret_patterns_only_reloads_on_version_change() {
    let mut cached_version = None;
    let mut cached_patterns: Vec<Regex> = Vec::new();
    let mut loads = 0usize;

    refresh_secret_patterns_if_needed(&mut cached_version, 1, &mut cached_patterns, || {
        loads += 1;
        vec![Regex::new("token").expect("regex")]
    });
    assert_eq!(loads, 1);

    refresh_secret_patterns_if_needed(&mut cached_version, 1, &mut cached_patterns, || {
        loads += 1;
        vec![Regex::new("never-called").expect("regex")]
    });
    assert_eq!(loads, 1);

    refresh_secret_patterns_if_needed(&mut cached_version, 2, &mut cached_patterns, || {
        loads += 1;
        vec![Regex::new("token2").expect("regex")]
    });
    assert_eq!(loads, 2);
}

#[test]
fn worker_preserves_order_and_flush_barrier() {
    let log_path = temp_log_path();
    let (tx, rx) = mpsc::sync_channel(8);

    let mut formatter = crate::log::formatter::LogFormatter::new();
    formatter.set_include_timestamp(false);
    formatter.set_include_break(false);
    let path_for_worker = log_path.clone();
    let file_factory: LogFileFactory = Arc::new(move || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_for_worker)
            .map_err(crate::log::LogError::from)
    });

    let worker = std::thread::spawn(move || {
        run_worker(rx, formatter, file_factory);
    });

    tx.send(SshLogCommand::Chunk(Arc::new("line-one\n".to_string()))).expect("send line one");
    tx.send(SshLogCommand::Chunk(Arc::new("line-two\n".to_string()))).expect("send line two");

    let (ack_tx, ack_rx) = mpsc::sync_channel(0);
    tx.send(SshLogCommand::Flush(ack_tx)).expect("send flush");
    assert!(ack_rx.recv().expect("flush ack").is_ok());

    drop(tx);
    worker.join().expect("worker should exit cleanly");
    let content = fs::read_to_string(&log_path).expect("read log file");
    assert!(content.contains("line-one"));
    assert!(content.contains("line-two"));
    assert!(content.find("line-one").expect("line one exists") < content.find("line-two").expect("line two exists"));
    let _ = fs::remove_file(log_path);
}

#[test]
fn worker_flush_writes_partial_tail_without_newline() {
    let log_path = temp_log_path();
    let (tx, rx) = mpsc::sync_channel(8);

    let mut formatter = crate::log::formatter::LogFormatter::new();
    formatter.set_include_timestamp(false);
    formatter.set_include_break(false);
    let path_for_worker = log_path.clone();
    let file_factory: LogFileFactory = Arc::new(move || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_for_worker)
            .map_err(crate::log::LogError::from)
    });

    let worker = std::thread::spawn(move || {
        run_worker(rx, formatter, file_factory);
    });

    tx.send(SshLogCommand::Chunk(Arc::new("partial-tail".to_string()))).expect("send partial tail");

    let (ack_tx, ack_rx) = mpsc::sync_channel(0);
    tx.send(SshLogCommand::Flush(ack_tx)).expect("send flush");
    assert!(ack_rx.recv().expect("flush ack").is_ok());

    drop(tx);
    worker.join().expect("worker should exit cleanly");
    let content = fs::read_to_string(&log_path).expect("read log file");
    assert!(content.contains("partial-tail"));
    let _ = fs::remove_file(log_path);
}
