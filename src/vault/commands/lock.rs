use super::VaultError;
use crate::log_debug;
use crate::vault::VaultManager;
use std::path::PathBuf;

pub fn run(vault_file: &PathBuf) -> Result<(), VaultError> {
    let service = "vault";
    let service_key = "vault_key";
    let user = vault_file.to_str().unwrap_or("default");

    println!("🔒 Locking vault...");
    log_debug!("Vault file locked: {}", vault_file.display());

    VaultManager::delete_keyring_entry(service, user)?;
    VaultManager::delete_keyring_entry(service_key, user)?;

    Ok(())
}
