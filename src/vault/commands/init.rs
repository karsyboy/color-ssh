/*
TODO:
    - Set defaults for both kdbx and key file and if the defaults are used add them to user config
    - Allow user to provide path for their own kdbx and key file and add them to user config
        - If the files already exist exit()
*/
use super::{KeepassVault, VaultError, VaultManager};
use crate::ui::Prompt;
use dirs::home_dir;
use secrecy::SecretBox;
use std::path::PathBuf;

pub fn run(vault_file: PathBuf, key_file: Option<PathBuf>) -> Result<(), VaultError> {
    // Check if the vault file exists
    if VaultManager::vault_exists(&vault_file) {
        return Err(VaultError::VaultAlreadyExists);
    }
    if VaultManager::vault_key_exists(&key_file) {
        return Err(VaultError::VaultAlreadyExists);
    }

    let prompt = Prompt::default();

    let use_password = prompt.yes_no_prompt("Do you want to use a password?", true);
    let use_key_file = prompt.yes_no_prompt("Do you want to use a key file?", false);

    let password = if use_password {
        SecretBox::new(Box::new(prompt.password_prompt().expect("Failed to get password.")))
    } else {
        SecretBox::new(Box::new(String::new()))
    };

    let key_file = if use_key_file {
        let key_file = KeepassVault::create_key_file(
            key_file.unwrap_or(
                home_dir()
                    .expect("Failed to get home directory.\r")
                    .join(".csh")
                    .join("vault")
                    .join("vault.key"),
            ),
        )?;
        Some(key_file)
    } else {
        None
    };

    // Get vault name from user
    let vault_name = prompt.validated_input_prompt("Enter the name of the vault", "^[a-zA-Z0-9_-]+$", "Vault name cannot be empty");

    let mut vault = KeepassVault::new(vault_file, Some(password), key_file);

    vault.set_key()?;
    vault.create()?;
    vault.db.meta.database_name = Some(vault_name);
    vault.create()?;

    Ok(())
}
