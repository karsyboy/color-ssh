use super::*;
use crate::auth::secret::ExposeSecret;
use crate::test::support::auth::TestVaultEnv;
use crate::validation::validate_vault_entry_name;

#[test]
fn validate_vault_entry_name_core_rules() {
    assert!(validate_vault_entry_name("shared"));
    assert!(validate_vault_entry_name("ok_1.2-3"));
    assert!(!validate_vault_entry_name("../bad"));
    assert!(!validate_vault_entry_name("with space"));
}

#[test]
fn vault_round_trip_and_entry_exists_core_behavior() {
    let env = TestVaultEnv::new("round_trip_core");
    let unlocked = env.init_and_unlock("master-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let secret = unlocked.get_secret("shared").expect("get secret");
    assert_eq!(secret.expose_secret(), "top-secret");
    assert!(entry_exists_with_paths(env.paths(), "shared").expect("entry exists"));
}

#[test]
fn wrong_master_password_and_uninitialized_list_entries_fail() {
    let env = TestVaultEnv::new("wrong_password_core");
    env.init("master-pass");

    assert!(matches!(
        unlock_with_password_and_paths(env.paths(), "wrong-pass"),
        Err(VaultError::InvalidMasterPassword)
    ));

    let missing_env = TestVaultEnv::new("list_uninitialized_core");
    assert!(matches!(list_entries_with_paths(missing_env.paths()), Err(VaultError::VaultNotInitialized)));
}

#[test]
fn rotate_master_password_preserves_encrypted_data() {
    let env = TestVaultEnv::new("rotate_core");
    let unlocked = env.init_and_unlock("old-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    rotate_master_password_with_paths(env.paths(), "old-pass", "new-pass").expect("rotate master password");

    assert!(matches!(
        unlock_with_password_and_paths(env.paths(), "old-pass"),
        Err(VaultError::InvalidMasterPassword)
    ));

    let unlocked_new = unlock_with_password_and_paths(env.paths(), "new-pass").expect("unlock with new password");
    assert_eq!(unlocked_new.get_secret("shared").expect("read secret").expose_secret(), "top-secret");
}
