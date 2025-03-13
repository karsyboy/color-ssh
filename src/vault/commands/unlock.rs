use super::{KeepassVault, VaultError, keyring};
use crate::{log_debug, ui};
use secrecy::{ExposeSecret, SecretBox};
use std::{ffi::OsStr, path::PathBuf};

pub fn run(vault_file: PathBuf, key_file: Option<PathBuf>) -> Result<KeepassVault, VaultError> {
    if !vault_file.exists() {
        return Err(VaultError::VaultNotFound(vault_file.display().to_string()));
    }

    let prompt = ui::Prompt::default();
    let service = "vault";
    let service_key = "vault_key";
    let vualt_user = vault_file.file_name().and_then(OsStr::to_str).unwrap_or_default();

    let keyring_entry_vault = keyring::get_keyring_entry(service, vualt_user)?;
    let keyring_entry_key = keyring::get_keyring_entry(service_key, vualt_user)?;

    log_debug!("Vault file: {}", vault_file.display());
    log_debug!("Key file: {}", key_file.clone().unwrap_or_default().display());
    log_debug!("Keyring entry vault: {:?}", keyring_entry_vault);
    log_debug!("Keyring entry key: {:?}", keyring_entry_key);
    log_debug!("password: {:?}", keyring_entry_vault.get_password());

    match keyring_entry_vault.get_password() {
        Ok(_) => {
            println!("🔓 Vault unlocked by keyring.");
            log_debug!("Vault unlocked by keyring.");

            let password = SecretBox::new(Box::new(keyring_entry_vault.get_password().unwrap_or_default()));
            let key_file = keyring_entry_key.get_password().ok().filter(|s| !s.is_empty()).map(PathBuf::from);

            let vault = KeepassVault::new(vault_file, password, key_file);

            return Ok(vault);
        }

        Err(_) => {
            println!("🔐 Unlocking vault...");
            log_debug!("Vault unlocked by user input.");

            if key_file.is_some() {
                println!("🔑 Using key file.");
                keyring_entry_key.set_password(key_file.clone().unwrap_or_default().to_str().unwrap_or(""))?;
            } else {
                println!("🔑 No key file provided.");
            }

            let password = SecretBox::new(Box::new(prompt.password_prompt()));
            keyring_entry_vault.set_password(&password.expose_secret())?;

            let key_file = keyring_entry_key.get_password().ok().filter(|s| !s.is_empty()).map(PathBuf::from);

            let vault = KeepassVault::new(vault_file, password, key_file);

            return Ok(vault);
        }
    };
}

// pub fn run(vault_file: PathBuf, key_file: Option<PathBuf>) -> Result<VaultManager, VaultError> {
//     if !vault_file.exists() {
//         return Err(VaultError::VaultNotFound(vault_file.display().to_string()));
//     }

//     let mut vault_manager = VaultManager::new();

//     let service = "vault";
//     let service_key = "vault_key";
//     let vault_file_name = vault_file.file_name().and_then(OsStr::to_str).unwrap_or_default();
//     let entry = keyring::get_keyring_entry(service, &vault_file_name)?;
//     let entry_key = keyring::get_keyring_entry(service_key, &vault_file_name)?;

//     log_debug!("Vault file: {}", vault_file.display());
//     log_debug!("Key file: {}", key_file.clone().unwrap_or_default().display());
//     log_debug!("Service: {}", service);
//     log_debug!("Vault File Name: {}", vault_file_name);
//     log_debug!("Entry: {:?}", entry);
//     log_debug!("Entry key: {:?}", entry_key);

//     match entry.get_password() {
//         Ok(_) => {
//             println!("🔓 Vault unlocked by keyring.");
//             log_debug!("Vault unlocked by keyring.");

//             vault_manager.set_vault_file_name(vault_file_name);
//             vault_manager.set_vault_path(vault_file.clone());

//             let key_file = entry_key.get_password().unwrap_or_default();
//             let key_file = PathBuf::from(key_file);
//             vault_manager.set_vault_key(key_file);

//             let password = SecretBox::new(Box::new(entry.get_password().unwrap_or_default()));
//             vault_manager.set_vault_password(password);

//             return Ok(vault_manager);
//         }

//         Err(_) => {
//             println!("🔐 Unlocking vault...");
//             log_debug!("Vault unlocked by user input.");

//             vault_manager.set_vault_file_name(vault_file_name);
//             vault_manager.set_vault_path(vault_file.clone());

//             if key_file.is_some() {
//                 println!("🔑 Using key file.");
//                 entry_key.set_password(key_file.clone().unwrap_or_default().to_str().unwrap_or(""))?;
//                 vault_manager.set_vault_key(key_file.clone().unwrap());
//             } else {
//                 println!("🔑 No key file provided.");
//             }

//             let password = VaultManager::get_password()?;
//             entry.set_password(&password.expose_secret())?;
//             vault_manager.set_vault_password(password);

//             let vault = vault_manager.get_vault_values();

//             log_debug!("Vault values: {:?}", vault);

//             return Ok(vault_manager);
//         }
//     };
// }
