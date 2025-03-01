use secrecy::SecretBox;

use super::VaultError;
use crate::vault::VaultManager;
use std::fs::File;
use std::path::PathBuf;

pub fn run(vault_file: &PathBuf, key_file: Option<&PathBuf>) -> Result<(), VaultError> {
    // Check if the vault file exists
    if VaultManager::vault_exists(vault_file) {
        return Err(VaultError::VaultAlreadyExists);
    }
    if VaultManager::vault_key_exists(key_file) {
        return Err(VaultError::VaultAlreadyExists);
    }

    let use_password = VaultManager::ask_yes_no("Do you want to use a password?", true);
    let use_key_file = VaultManager::ask_yes_no("Do you want to use a key file?", false);

    let password = if use_password {
        VaultManager::get_password()?
    } else {
        SecretBox::new(Box::new(String::new()))
    };

    let key_file: Option<&PathBuf> = if use_key_file {
        let key_file = VaultManager::create_key_file(key_file)?;
        Some(&key_file)
    } else {
        None
    };

    // Get vault name from user
    let vault_name = VaultManager::get_vault_name()?;

    let mut vault = VaultManager::new_keepass_db().map_err(|err| {
        println!("Failed to create new KeePass database: {}", err);
        VaultError::KeepassError(err)
    })?;

    vault.meta.database_name = Some(vault_name);

    let vault_key = VaultManager::new_vault_key(key_file.as_deref(), password).map_err(|err| {
        println!("Failed to create new vault key: {}", err);
        VaultError::KeyFileCreationFailed
    })?;

    vault.save(&mut File::create(vault_file)?, vault_key)?;

    Ok(())
}
