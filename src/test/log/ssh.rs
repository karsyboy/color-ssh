use super::{LogFileFactory, SecretPatternSource, SshLogCommand, extract_complete_lines, run_worker, sanitize_line};
use crate::log::LogError;
use crate::test::support::fs::TestWorkspace;
use regex::Regex;
use std::path::Path;

fn create_private_directory(path: &Path) -> Result<(), LogError> {
    Ok(crate::platform::create_private_directory(path, 0o700)?)
}

fn open_private_append_file(path: &Path) -> Result<std::fs::File, LogError> {
    Ok(crate::platform::open_private_append_file(path, 0o600)?)
}
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

fn spawn_worker(log_path: std::path::PathBuf) -> (mpsc::SyncSender<SshLogCommand>, JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(8);

    let mut formatter = crate::log::formatter::LogFormatter::new();
    formatter.set_include_timestamp(false);
    formatter.set_include_break(false);

    let file_factory: LogFileFactory = Arc::new(move || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(crate::log::LogError::from)
    });

    let worker = std::thread::spawn(move || run_worker(rx, formatter, file_factory, SecretPatternSource::Fixed(Vec::new())));
    (tx, worker)
}

#[test]
fn sanitize_line_and_extract_complete_lines_handle_redaction_and_partial_buffers() {
    let secrets = vec![Regex::new("token=\\w+").expect("regex compiles")];
    let sanitized = sanitize_line("\x1b[31mtoken=abc123\x1b[0m ok", &secrets);
    assert_eq!(sanitized, "[REDACTED] ok");

    let mut buffer = "one\ntwo\npartial".to_string();
    let lines = extract_complete_lines(&mut buffer);
    assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
    assert_eq!(buffer, "partial");
}

#[test]
fn worker_flush_writes_chunks_in_order() {
    let root = TestWorkspace::new("log", "ssh_worker").expect("temp workspace");
    let log_path = root.join("session.log");
    let (tx, worker) = spawn_worker(log_path.clone());

    tx.send(SshLogCommand::Chunk(Arc::new("line-one\n".to_string()))).expect("send line one");
    tx.send(SshLogCommand::Chunk(Arc::new("line-two\n".to_string()))).expect("send line two");

    let (ack_tx, ack_rx) = mpsc::sync_channel(0);
    tx.send(SshLogCommand::Flush(ack_tx)).expect("send flush");
    assert!(ack_rx.recv().expect("flush ack").is_ok());

    drop(tx);
    worker.join().expect("worker should exit cleanly");

    let content = fs::read_to_string(&log_path).expect("read log file");
    assert!(content.find("line-one").expect("line one exists") < content.find("line-two").expect("line two exists"));
}

#[test]
fn private_log_file_permissions_are_restrictive() {
    let root = TestWorkspace::new("log", "ssh_permissions").expect("temp workspace");
    let log_dir = root.join("ssh_sessions");
    let log_path = log_dir.join("session.log");

    create_private_directory(&log_dir).expect("create private ssh log directory");
    let _file = open_private_append_file(&log_path).expect("create private ssh log file");

    let dir_mode = fs::metadata(&log_dir).expect("directory metadata").permissions().mode() & 0o777;
    let file_mode = fs::metadata(&log_path).expect("file metadata").permissions().mode() & 0o777;

    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);
}
