use super::*;

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
