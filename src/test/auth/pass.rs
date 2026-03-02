use super::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(prefix: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("cossh_pass_{prefix}_{nanos}_{serial}"))
}

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
    cache.seed("shared", "cached-secret");

    let result = resolve_pass_key("shared", &mut cache);
    assert_eq!(result, PassResolveResult::Ready("cached-secret".to_string()));
}

#[test]
fn resolve_pass_key_for_tui_uses_cache_without_prompt() {
    let mut cache = PassCache::default();
    cache.seed("shared", "cached-secret");

    let result = resolve_pass_key_for_tui("shared", &mut cache);
    assert_eq!(result, PassPromptStatus::Ready("cached-secret".to_string()));
}

#[test]
fn resolve_pass_key_rejects_invalid_names() {
    let mut cache = PassCache::default();
    let result = resolve_pass_key("../bad", &mut cache);
    assert_eq!(result, PassResolveResult::Fallback(PassFallbackReason::InvalidPassKeyName));
}

#[test]
fn submit_tui_passphrase_uses_cache_without_shelling_out() {
    let mut cache = PassCache::default();
    cache.seed("shared", "cached-secret");

    let result = submit_tui_passphrase("shared", "ignored", &mut cache);
    assert_eq!(result, PassPromptSubmitResult::Ready("cached-secret".to_string()));
}

#[test]
fn confirm_password_entry_rejects_empty_and_mismatch() {
    assert!(matches!(
        confirm_password_entry(String::new(), String::new()),
        Err(PassCreateError::EmptyPassword)
    ));
    assert!(matches!(
        confirm_password_entry("abc".to_string(), "xyz".to_string()),
        Err(PassCreateError::PasswordMismatch)
    ));
    let confirmed = confirm_password_entry("abc".to_string(), "abc".to_string()).expect("password should match");
    assert_eq!(confirmed, "abc");
}

#[test]
fn parse_overwrite_confirmation_accepts_yes_values() {
    assert!(parse_overwrite_confirmation("y"));
    assert!(parse_overwrite_confirmation("YES"));
    assert!(!parse_overwrite_confirmation("n"));
    assert!(!parse_overwrite_confirmation(""));
}

#[test]
fn create_pass_key_with_hooks_declines_existing_file_without_encrypting() {
    let root = temp_path("overwrite_decline");
    let output_path = root.join("keys").join("device.gpg");
    fs::create_dir_all(output_path.parent().expect("parent")).expect("create keys");
    fs::write(&output_path, b"existing").expect("write existing");

    let mut encrypt_called = false;
    let result = create_pass_key_with_hooks(
        "device",
        output_path.clone(),
        |_path| Ok(false),
        || Ok("secret".to_string()),
        |_path, _payload| {
            encrypt_called = true;
            Ok(())
        },
    );

    assert!(matches!(result, Err(PassCreateError::OverwriteDeclined)));
    assert!(!encrypt_called);
    let persisted = fs::read(&output_path).expect("existing content");
    assert_eq!(persisted, b"existing");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn create_pass_key_with_hooks_passes_raw_password_bytes_without_newline() {
    let root = temp_path("create_success");
    let output_path = root.join("keys").join("device.gpg");
    let mut seen_payload = Vec::new();

    let result = create_pass_key_with_hooks(
        "device",
        output_path.clone(),
        |_path| Ok(true),
        || Ok("top-secret".to_string()),
        |path, payload| {
            seen_payload.extend_from_slice(payload);
            fs::write(path, b"encrypted").map_err(PassCreateError::Io)?;
            Ok(())
        },
    )
    .expect("create pass key");

    assert_eq!(result, output_path);
    assert_eq!(seen_payload, b"top-secret");

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn create_pass_key_with_hooks_enforces_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_path("permissions");
    let output_path = root.join("keys").join("device.gpg");

    create_pass_key_with_hooks(
        "device",
        output_path.clone(),
        |_path| Ok(true),
        || Ok("secret".to_string()),
        |path, _payload| {
            fs::write(path, b"encrypted").map_err(PassCreateError::Io)?;
            Ok(())
        },
    )
    .expect("create pass key");

    let dir_mode = fs::metadata(output_path.parent().expect("parent")).expect("dir metadata").permissions().mode() & 0o777;
    let file_mode = fs::metadata(&output_path).expect("file metadata").permissions().mode() & 0o777;

    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);

    let _ = fs::remove_dir_all(root);
}
