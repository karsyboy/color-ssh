use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SecretWrapper {
    #[serde(with = "serde_sensitive_string")]
    secret: SensitiveString,
}

#[test]
fn sensitive_buffer_edit_operations_and_debug_redaction() {
    let mut buffer = SensitiveBuffer::new();
    for (index, ch) in "secret".chars().enumerate() {
        buffer.insert_char(index, ch);
    }
    buffer.insert_char(3, 'X');
    assert_eq!(buffer.as_str().expect("valid utf8"), "secXret");
    assert_eq!(format!("{buffer:?}"), "[REDACTED]");
}

#[test]
fn serde_sensitive_string_round_trip_keeps_secret_value_without_debug_leak() {
    let encoded = br#"{"secret":"top-secret"}"#;

    let decoded: SecretWrapper = serde_json::from_slice(encoded).expect("decode secret wrapper");
    assert_eq!(decoded.secret.expose_secret(), "top-secret");
    assert!(!format!("{decoded:?}").contains("top-secret"));
}
