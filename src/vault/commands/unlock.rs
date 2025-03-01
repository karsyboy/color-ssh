use super::VaultError;
use crate::log_debug;
use crate::vault::VaultManager;
use secrecy::{ExposeSecret, SecretBox};
use std::ffi::OsStr;
use std::path::PathBuf;

pub fn run(vault_path: &PathBuf, key_file: Option<&PathBuf>) -> Result<VaultManager, VaultError> {
    let mut vault_manager = VaultManager::new();

    let service = "vault";
    let service_key = "vault_key";
    let vault_file_name = vault_path
        .file_name()
        .and_then(OsStr::to_str)
        .expect("Failed to get file name");
    let entry = VaultManager::get_keyring_entry(service, &vault_file_name)
        .expect("Failed to get keyring entry");
    let entry_key = VaultManager::get_keyring_entry(service_key, &vault_file_name)?;

    log_debug!("Vault file: {}", vault_path.display());
    log_debug!(
        "Key file: {}",
        key_file.unwrap_or(&PathBuf::new()).display()
    );
    log_debug!("Service: {}", service);
    log_debug!("Vault File Name: {}", vault_file_name);
    log_debug!("Entry: {:?}", entry);
    log_debug!("Entry key: {:?}", entry_key);

    match entry.get_password() {
        Ok(_) => {
            println!("🔓 Vault unlocked by keyring.");
            log_debug!("Vault unlocked by keyring.");

            vault_manager.set_vault_file_name(vault_file_name);
            vault_manager.set_vault_path(vault_path.clone());

            let key_file = entry_key.get_password().unwrap_or_default();
            let key_file = PathBuf::from(key_file);
            vault_manager.set_vault_key(key_file);

            let password = SecretBox::new(Box::new(entry.get_password().unwrap_or_default()));
            vault_manager.set_vault_password(password);

            return Ok(vault_manager);
        }

        Err(_) => {
            println!("🔐 Unlocking vault...");
            log_debug!("Vault unlocked by user input.");

            vault_manager.set_vault_file_name(vault_file_name);
            vault_manager.set_vault_path(vault_path.clone());

            if key_file.is_some() {
                println!("🔑 Using key file.");
                entry_key
                    .set_password(key_file.unwrap().to_str().expect("Failed to set keyfile"))?;
                vault_manager.set_vault_key(key_file.unwrap().clone());
            } else {
                println!("🔑 No key file provided.");
            }

            let password = VaultManager::get_password()?;
            entry.set_password(&password.expose_secret())?;
            vault_manager.set_vault_password(password);

            let vault = vault_manager.get_vault_values();

            log_debug!("Vault values: {:?}", vault);

            return Ok(vault_manager);
        }
    };
}
