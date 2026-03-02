use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_paths(prefix: &str) -> VaultPaths {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    VaultPaths::new(std::env::temp_dir().join(format!("cossh_vault_{prefix}_{nanos}_{serial}")))
}

#[test]
fn validate_entry_name_allows_safe_characters_only() {
    assert!(validate_entry_name("shared"));
    assert!(validate_entry_name("ok_1.2-3"));
    assert!(!validate_entry_name("../bad"));
    assert!(!validate_entry_name("with space"));
}

#[test]
fn initialize_unlock_store_and_read_entry_round_trips() {
    let paths = temp_paths("round_trip");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let secret = unlocked.get_secret("shared").expect("get secret");
    assert_eq!(secret, "top-secret");

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn wrong_master_password_fails_to_unlock() {
    let paths = temp_paths("wrong_password");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");

    let result = unlock_with_password_and_paths(&paths, "wrong-pass");
    assert!(matches!(result, Err(VaultError::InvalidMasterPassword)));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn rotate_master_password_rewraps_data_key() {
    let paths = temp_paths("rotate");
    initialize_vault_with_paths(&paths, "old-pass").expect("initialize vault");
    let unlocked = unlock_with_password_and_paths(&paths, "old-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    rotate_master_password_with_paths(&paths, "old-pass", "new-pass").expect("rotate master password");

    assert!(matches!(
        unlock_with_password_and_paths(&paths, "old-pass"),
        Err(VaultError::InvalidMasterPassword)
    ));
    let unlocked = unlock_with_password_and_paths(&paths, "new-pass").expect("unlock with new password");
    assert_eq!(unlocked.get_secret("shared").expect("read secret"), "top-secret");

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn list_entries_returns_sorted_entry_names() {
    let paths = temp_paths("list_entries");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("z_last", "top-secret").expect("store z entry");
    unlocked.store_secret("a_first", "top-secret").expect("store a entry");
    unlocked.store_secret("middle_1", "top-secret").expect("store middle entry");

    let entries = list_entries_with_paths(&paths).expect("list entries");
    assert_eq!(entries, vec!["a_first".to_string(), "middle_1".to_string(), "z_last".to_string()]);

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn list_entries_requires_initialized_vault() {
    let paths = temp_paths("list_uninitialized");

    let result = list_entries_with_paths(&paths);
    assert!(matches!(result, Err(VaultError::VaultNotInitialized)));

    let _ = fs::remove_dir_all(paths.base_dir());
}
