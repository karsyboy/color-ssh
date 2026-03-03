use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SecretWrapper {
    #[serde(with = "serde_sensitive_string")]
    secret: SensitiveString,
}

#[test]
fn sensitive_buffer_edit_operations_preserve_content() {
    let mut buffer = SensitiveBuffer::new();
    buffer.insert_char(0, 's');
    buffer.insert_char(1, 'e');
    buffer.insert_char(2, 'c');
    buffer.insert_char(3, 'r');
    buffer.insert_char(4, 'e');
    buffer.insert_char(5, 't');
    buffer.insert_char(3, 'X');

    assert_eq!(buffer.as_str().expect("valid utf8"), "secXret");

    let cursor = buffer.backspace_char(4);
    assert_eq!(cursor, 3);
    assert_eq!(buffer.as_str().expect("valid utf8"), "secret");

    let cursor = buffer.delete_char(1);
    assert_eq!(cursor, 1);
    assert_eq!(buffer.as_str().expect("valid utf8"), "scret");
}

#[test]
fn sensitive_buffer_debug_is_redacted() {
    let mut buffer = SensitiveBuffer::new();
    for (idx, ch) in "secret".chars().enumerate() {
        buffer.insert_char(idx, ch);
    }

    assert_eq!(format!("{buffer:?}"), "[REDACTED]");
}

#[test]
fn serde_sensitive_string_round_trips_owned_strings() {
    let encoded = br#"{"secret":"top-secret"}"#;
    let decoded: SecretWrapper = serde_json::from_slice(encoded).expect("decode secret wrapper");

    assert_eq!(decoded.secret.expose_secret(), "top-secret");

    let reencoded = serde_json::to_string(&decoded).expect("encode secret wrapper");
    assert!(reencoded.contains("top-secret"));
    assert!(!format!("{decoded:?}").contains("top-secret"));
}
