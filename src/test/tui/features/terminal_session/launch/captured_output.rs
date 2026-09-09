use super::*;
use portable_pty::Child as PtyChild;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn tui_sessions_reap_exited_children_while_manager_remains_running() {
    let mut exited_states = Vec::new();
    let mut pids = Vec::new();

    for _ in 0..3 {
        let child_process = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch local mock session");
        pids.push(child_process.id());
        let child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>> = Arc::new(Mutex::new(Box::new(child_process)));
        let exited = Arc::new(Mutex::new(false));
        spawn_terminal_session_exit_watcher(child, Arc::clone(&exited)).expect("start session exit watcher");
        exited_states.push(exited);
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    while exited_states.iter().any(|exited| !*exited.lock().expect("exited state")) && Instant::now() < deadline {
        thread::yield_now();
    }

    assert!(
        exited_states.iter().all(|exited| *exited.lock().expect("exited state")),
        "mock sessions did not report process exit"
    );
    let reaped = pids
        .into_iter()
        .map(|pid| {
            let result = unsafe { nix::libc::waitpid(pid as nix::libc::pid_t, std::ptr::null_mut(), nix::libc::WNOHANG) };
            result == -1 && io::Error::last_os_error().raw_os_error() == Some(nix::libc::ECHILD)
        })
        .collect::<Vec<_>>();
    assert!(reaped.iter().all(|reaped| *reaped), "not every mock session child was reaped: {reaped:?}");
}

#[test]
fn startup_only_rdp_launch_payload_closes_writer_after_write() {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let dropped = Arc::new(AtomicBool::new(false));
    let writer = TrackingWriter {
        bytes: bytes.clone(),
        dropped: dropped.clone(),
    };
    let payload = crate::auth::secret::sensitive_string("/u:alice\n/p:super-secret");

    write_startup_payload_and_close_stdin(Box::new(writer), Some(&payload)).expect("write startup payload");

    assert_eq!(
        String::from_utf8(bytes.lock().expect("payload bytes").clone()).expect("payload should be utf-8"),
        "/u:alice\n/p:super-secret"
    );
    assert!(dropped.load(Ordering::Relaxed));
}

#[test]
fn vault_backed_rdp_launch_uses_captured_output_mode() {
    assert_eq!(
        rdp_session_launch_mode(crate::process::RdpLaunchMode::CapturedOutput),
        RdpSessionLaunchMode::CapturedOutput
    );
}

#[test]
fn prompt_backed_rdp_launch_uses_pty_mode() {
    assert_eq!(rdp_session_launch_mode(crate::process::RdpLaunchMode::Pty), RdpSessionLaunchMode::Pty);
}

#[test]
fn captured_output_newlines_expand_to_crlf() {
    let mut normalizer = CapturedOutputNewlineNormalizer::default();

    let normalized = normalize_captured_output_chunk(&mut normalizer, b"first line\nsecond line\n");

    assert_eq!(normalized, b"first line\r\nsecond line\r\n");
}

#[test]
fn captured_output_newlines_preserve_chunked_crlf_sequences() {
    let mut normalizer = CapturedOutputNewlineNormalizer::default();

    let first_chunk = normalize_captured_output_chunk(&mut normalizer, b"first line\r");
    let second_chunk = normalize_captured_output_chunk(&mut normalizer, b"\nsecond line\n");

    assert_eq!(first_chunk, b"first line\r");
    assert_eq!(second_chunk, b"\nsecond line\r\n");
}
