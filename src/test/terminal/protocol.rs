use super::{KeyCode, KeyEvent, KeyModifiers, encode_key_event_bytes};

#[test]
fn function_keys_encode_as_xterm_sequences() {
    let expected: [&[u8]; 12] = [
        b"\x1bOP",
        b"\x1bOQ",
        b"\x1bOR",
        b"\x1bOS",
        b"\x1b[15~",
        b"\x1b[17~",
        b"\x1b[18~",
        b"\x1b[19~",
        b"\x1b[20~",
        b"\x1b[21~",
        b"\x1b[23~",
        b"\x1b[24~",
    ];

    for (number, expected) in (1..=12).zip(expected) {
        assert_eq!(
            encode_key_event_bytes(KeyEvent::new(KeyCode::F(number), KeyModifiers::NONE)).as_deref(),
            Some(expected),
            "F{number}"
        );
    }
}

#[test]
fn function_keys_encode_xterm_modifier_parameters() {
    let cases = [
        (KeyCode::F(1), KeyModifiers::SHIFT, b"\x1b[1;2P".as_slice()),
        (KeyCode::F(2), KeyModifiers::ALT, b"\x1b[1;3Q".as_slice()),
        (KeyCode::F(5), KeyModifiers::CONTROL, b"\x1b[15;5~".as_slice()),
        (
            KeyCode::F(12),
            KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL,
            b"\x1b[24;8~".as_slice(),
        ),
    ];

    for (code, modifiers, expected) in cases {
        assert_eq!(encode_key_event_bytes(KeyEvent::new(code, modifiers)).as_deref(), Some(expected));
    }
}

#[test]
fn unsupported_function_key_number_is_not_encoded() {
    assert!(encode_key_event_bytes(KeyEvent::new(KeyCode::F(13), KeyModifiers::NONE)).is_none());
}
