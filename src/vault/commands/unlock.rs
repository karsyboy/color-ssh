use super::VaultError;
use crate::log_debug;
use crate::vault::VaultManager;
use std::path::PathBuf;

pub fn run(vault_file: &PathBuf, key_file: Option<PathBuf>) -> Result<(), VaultError> {
    let mut vault_manager = VaultManager::new();

    let service = "vault";
    let service_key = "vault_key";
    let user = vault_file.to_str().unwrap_or("default");
    let entry = VaultManager::get_keyring_entry(service, user)?;
    let entry_key = VaultManager::get_keyring_entry(service_key, user)?;

    log_debug!("Vault file: {}", vault_file.display());
    log_debug!("Key file: {:?}", key_file);
    log_debug!("Service: {}", service);
    log_debug!("User: {}", user);
    log_debug!("Entry: {:?}", entry);
    log_debug!("Entry key: {:?}", entry_key);

    match entry.get_password() {
        Ok(_) => {
            println!("🔓 Vault unlocked by keyring.");
            log_debug!("Vault unlocked by keyring.");
            let password = entry.get_password().unwrap_or_default();
            vault_manager.set_vault_password(password);

            let key_file = entry_key.get_password().unwrap_or_default();
            let key_file = PathBuf::from(key_file);
            vault_manager.set_vault_key(key_file);
        }

        Err(_) => {
            println!("🔐 Unlocking vault...");
            log_debug!("Vault unlocked by user input.");
            let password = VaultManager::get_password()?;
            entry.set_password(&password)?;
            vault_manager.set_vault_password(password);

            if let Some(key_file) = key_file {
                entry_key.set_password(key_file.to_str().unwrap_or(""))?;
                vault_manager.set_vault_key(key_file);
            }
        }
    };

    Ok(())
}
