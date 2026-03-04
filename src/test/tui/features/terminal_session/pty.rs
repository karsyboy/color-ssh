use super::{encode_key_event_bytes, normalize_managed_output_newlines};
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
fn normalize_managed_output_newlines_rewrites_lf_to_crlf() {
    let mut previous_ended_with_cr = false;
    let normalized = normalize_managed_output_newlines(b"first\nsecond\n", &mut previous_ended_with_cr);

    assert_eq!(normalized, b"first\r\nsecond\r\n");
    assert!(!previous_ended_with_cr);
}

#[test]
fn normalize_managed_output_newlines_preserves_existing_crlf_across_chunks() {
    let mut previous_ended_with_cr = false;

    let first = normalize_managed_output_newlines(b"first line\r", &mut previous_ended_with_cr);
    let second = normalize_managed_output_newlines(b"\nsecond line\n", &mut previous_ended_with_cr);

    assert_eq!(first, b"first line\r");
    assert_eq!(second, b"\nsecond line\r\n");
    assert!(!previous_ended_with_cr);
}
