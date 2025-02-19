use super::VaultError;
use crate::log_debug;
use crate::vault::VaultManager;
use secrecy::{ExposeSecret, SecretBox};
use std::fs;
use std::path::PathBuf;

pub fn run(vault_file: &PathBuf, key_file: Option<&PathBuf>) -> Result<(), VaultError> {
    let mut vault_manager = VaultManager::new();

    let service = "vault";
    let service_key = "vault_key";
    let vault_hash = format!("{:x}", md5::compute(fs::read_to_string(&vault_file)?));
    let entry = VaultManager::get_keyring_entry(service, &vault_hash.as_str())?;
    let entry_key = VaultManager::get_keyring_entry(service_key, &vault_hash.as_str())?;

    log_debug!("Vault file: {}", vault_file.display());
    log_debug!("Key file: {}", key_file.unwrap_or(&PathBuf::new()).display());
    log_debug!("Service: {}", service);
    log_debug!("Vault Hash: {}", vault_hash);
    log_debug!("Entry: {:?}", entry);
    log_debug!("Entry key: {:?}", entry_key);

    match entry.get_password() {
        Ok(_) => {
            println!("🔓 Vault unlocked by keyring.");
            log_debug!("Vault unlocked by keyring.");

            vault_manager.set_vault_hash(vault_hash.clone());
            vault_manager.set_vault_file(vault_file.clone());

            let key_file = entry_key.get_password().unwrap_or_default();
            let key_file = PathBuf::from(key_file);
            vault_manager.set_vault_key(key_file);

            let password = SecretBox::new(Box::new(entry.get_password().unwrap_or_default()));
            vault_manager.set_vault_password(password);
            
        }

        Err(_) => {
            println!("🔐 Unlocking vault...");
            log_debug!("Vault unlocked by user input.");

            vault_manager.set_vault_hash(vault_hash.clone());
            vault_manager.set_vault_file(vault_file.clone());

            if key_file.is_some() {
                println!("🔑 Using key file.");
                entry_key.set_password(key_file.unwrap().to_str().expect("Failed to set keyfile"))?;
                vault_manager.set_vault_key(key_file.unwrap().clone());
            } else {
                println!("🔑 No key file provided.");
            }

            let password = VaultManager::get_password()?;
            entry.set_password(&password.expose_secret())?;
            vault_manager.set_vault_password(password);

            let vault = vault_manager.get_vault_values();

            log_debug!("Vault values: {:?}", vault);
                        
        }
    };

    Ok(())
}
