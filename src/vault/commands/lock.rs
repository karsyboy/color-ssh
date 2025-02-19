use super::VaultError;
use crate::log_debug;
use crate::vault::VaultManager;
use std::path::PathBuf;
use std::fs;

pub fn run(vault_file: &PathBuf) -> Result<(), VaultError> {
    let service = "vault";
    let service_key = "vault_key";
    let vault_hash = format!("{:x}", md5::compute(fs::read_to_string(&vault_file)?));

    println!("🔒 Locking vault...");
    log_debug!("Vault file locked: {}", vault_file.display());
    log_debug!("Vault Hash: {}", vault_hash);

    VaultManager::delete_keyring_entry(service, &vault_hash.as_str())?;
    VaultManager::delete_keyring_entry(service_key, &vault_hash.as_str())?;

    Ok(())
}
