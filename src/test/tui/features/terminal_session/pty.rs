use super::{encode_key_event_bytes, flush_pending_initial_line, merge_fallback_notice, suppress_initial_password_echo};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn encode_key_event_bytes_ctrl_char() {
    let key = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL);
    assert_eq!(encode_key_event_bytes(key), Some(vec![3]));
}

#[test]
fn encode_key_event_bytes_ctrl_bracket_variants() {
    let open = KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL);
    let backslash = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
    let close = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);
    let at = KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL);

    assert_eq!(encode_key_event_bytes(open), Some(vec![27]));
    assert_eq!(encode_key_event_bytes(backslash), Some(vec![28]));
    assert_eq!(encode_key_event_bytes(close), Some(vec![29]));
    assert_eq!(encode_key_event_bytes(at), Some(vec![0]));
}

#[test]
fn encode_key_event_bytes_alt_char_prefixes_escape() {
    let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);
    assert_eq!(encode_key_event_bytes(key), Some(vec![0x1b, b'f']));
}

#[test]
fn encode_key_event_bytes_arrow() {
    let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(encode_key_event_bytes(key), Some(b"\x1b[A".to_vec()));
}

#[test]
fn encode_key_event_bytes_alt_arrow_prefixes_escape() {
    let key = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
    assert_eq!(encode_key_event_bytes(key), Some(b"\x1b\x1b[A".to_vec()));
}

#[test]
fn encode_key_event_bytes_shift_arrow_preserves_shift_modifier() {
    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT);
    assert_eq!(encode_key_event_bytes(key), Some(b"\x1b[1;2D".to_vec()));
}

#[test]
fn encode_key_event_bytes_shift_pageup_preserves_shift_modifier() {
    let key = KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT);
    assert_eq!(encode_key_event_bytes(key), Some(b"\x1b[5;2~".to_vec()));
}

#[test]
fn merge_fallback_notice_appends_new_message() {
    let merged = merge_fallback_notice(Some("first".to_string()), "second".to_string());
    assert_eq!(merged, "first second");
}

#[test]
fn suppress_initial_password_echo_drops_matching_first_line() {
    let mut pending = Vec::new();
    let mut initial_password = Some("top-secret".to_string());

    let output = suppress_initial_password_echo(b"top-secret\r\nbanner\r\n", &mut pending, &mut initial_password);

    assert_eq!(output, b"banner\r\n");
    assert!(pending.is_empty());
    assert!(initial_password.is_none());
}

#[test]
fn suppress_initial_password_echo_preserves_non_matching_first_line() {
    let mut pending = Vec::new();
    let mut initial_password = Some("top-secret".to_string());

    let output = suppress_initial_password_echo(b"hello\r\nbanner\r\n", &mut pending, &mut initial_password);

    assert_eq!(output, b"hello\r\nbanner\r\n");
    assert!(pending.is_empty());
    assert!(initial_password.is_none());
}

#[test]
fn flush_pending_initial_line_drops_unterminated_password_echo() {
    let mut pending = b"top-secret".to_vec();
    let mut initial_password = Some("top-secret".to_string());

    let output = flush_pending_initial_line(&mut pending, &mut initial_password);

    assert!(output.is_empty());
    assert!(pending.is_empty());
    assert!(initial_password.is_none());
}
