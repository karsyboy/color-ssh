use super::*;
use std::path::Path;

#[test]
fn validate_pass_key_name_allows_safe_characters_only() {
    assert!(validate_pass_key_name("abc"));
    assert!(validate_pass_key_name("abc_123.test-name"));

    assert!(!validate_pass_key_name(""));
    assert!(!validate_pass_key_name("../secret"));
    assert!(!validate_pass_key_name("has space"));
    assert!(!validate_pass_key_name("semi;colon"));
}

#[test]
fn extract_password_uses_first_line_and_trims_line_endings_only() {
    assert_eq!(extract_password_from_plaintext(b" top-secret  \nsecond-line").as_deref(), Some(" top-secret  "));
    assert_eq!(extract_password_from_plaintext(b"abc\r\nnext").as_deref(), Some("abc"));
    assert_eq!(extract_password_from_plaintext(b"\nnext"), None);
}

#[test]
fn pass_key_path_uses_color_ssh_keys_directory() {
    let path = pass_key_path("lab").expect("home dir");
    let rendered = path.to_string_lossy();
    assert!(rendered.ends_with("/.color-ssh/keys/lab.gpg"));
}

#[test]
fn decrypt_with_retry_retries_then_succeeds() {
    let mut attempts = 0usize;
    let result = decrypt_with_retry(Path::new("/tmp/ignored"), |_| {
        attempts += 1;
        if attempts < 3 { Err(DecryptError::Retryable) } else { Ok("ok".to_string()) }
    });

    assert_eq!(result, Ok("ok".to_string()));
    assert_eq!(attempts, 3);
}

#[test]
fn decrypt_with_retry_stops_after_three_attempts() {
    let mut attempts = 0usize;
    let result = decrypt_with_retry(Path::new("/tmp/ignored"), |_| {
        attempts += 1;
        Err(DecryptError::Retryable)
    });

    assert_eq!(result, Err(PassFallbackReason::DecryptFailedAfterRetries));
    assert_eq!(attempts, 3);
}

#[test]
fn decrypt_with_retry_missing_gpg_fails_immediately() {
    let mut attempts = 0usize;
    let result = decrypt_with_retry(Path::new("/tmp/ignored"), |_| {
        attempts += 1;
        Err(DecryptError::MissingGpg)
    });

    assert_eq!(result, Err(PassFallbackReason::MissingGpg));
    assert_eq!(attempts, 1);
}

#[test]
fn resolve_pass_key_uses_cache_without_decrypting_again() {
    let mut cache = PassCache::default();
    cache.passwords.insert("shared".to_string(), "cached-secret".to_string());

    let result = resolve_pass_key("shared", &mut cache);
    assert_eq!(result, PassResolveResult::Ready("cached-secret".to_string()));
}

#[test]
fn resolve_pass_key_rejects_invalid_names() {
    let mut cache = PassCache::default();
    let result = resolve_pass_key("../bad", &mut cache);
    assert_eq!(result, PassResolveResult::Fallback(PassFallbackReason::InvalidPassKeyName));
}
