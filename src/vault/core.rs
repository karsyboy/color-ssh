/*
TODO:
    - for linux something like gnome-keyring may be required now.
*/

use keyring::Entry;
use rpassword::prompt_password;
// use secrecy::{ExposeSecret, SecretBox};
use std::path::PathBuf;

// use keepass::{
//     db::{Entry, Group, Node},
//     error::DatabaseOpenError,
//     Database, DatabaseKey,
// };

use super::VaultError;
use crate::cli::VaultArgs;
use crate::vault::commands::{lock_vault, unlock_vault};

pub struct VaultManager {
    vault_name: String,
    vault_path: PathBuf,
    vault_key: Option<PathBuf>,
    vault_password: String,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            vault_name: String::new(),
            vault_path: PathBuf::new(),
            vault_key: Some(PathBuf::new()),
            vault_password: String::new(),
        }
    }

    pub fn start(vault_commands: VaultArgs) {
        match vault_commands {
            VaultArgs::Add {
                entry_name,
                key_file,
                use_password,
            } => {
                println!("Adding entry: {}", entry_name);
                println!("Key file: {:?}", key_file.unwrap_or_default());
                println!("Use password: {}", use_password);
                // add::run(entry_name, key_file, use_password);
            }
            VaultArgs::Delete { entry_name } => {
                println!("Deleting entry: {}", entry_name);
                // del::run(entry_name);
            }
            VaultArgs::Init { vault_name } => {
                println!("Initializing vault: {}", vault_name);
                // init::run(vault_name);
            }
            VaultArgs::Show { entry_name } => {
                println!("Showing entry: {}", entry_name);
                // show::run(entry_name);
            }
            VaultArgs::Lock { vault_file } => {
                let _ = lock_vault(&vault_file);

                // Insert logic for locking the vault here.
            }
            VaultArgs::Unlock {
                vault_file,
                key_file,
            } => {
                let _ = unlock_vault(&vault_file, key_file);
            }
        }
    }

    pub fn set_vault_name(&mut self, name: String) {
        self.vault_name = name;
    }

    pub fn get_vault_name(&self) -> &String {
        &self.vault_name
    }

    pub fn set_vault_path(&mut self, path: PathBuf) {
        self.vault_path = path;
    }

    pub fn get_vault_path(&self) -> &PathBuf {
        &self.vault_path
    }

    pub fn set_vault_key(&mut self, key: PathBuf) {
        self.vault_key = Some(key);
    }

    pub fn get_vault_key(&self) -> Option<&PathBuf> {
        self.vault_key.as_ref()
    }

    pub fn set_vault_password(&mut self, password: String) {
        self.vault_password = password;
    }

    pub fn get_vault_password(&self) -> &String {
        &self.vault_password
    }

    pub fn get_password() -> Result<String, VaultError> {
        let password = prompt_password("Enter your password: ")?;
        let verified_password = prompt_password("Verify your password: ")?;
        if password != verified_password {
            return Err(VaultError::InvalidPassword);
        }

        if password.is_empty() {
            return Err(VaultError::EmptyPassword);
        }

        Ok(password)
    }

    pub fn create_keyring_entry(
        service: &str,
        user: &str,
        password: Option<&str>,
        secret: Option<&str>,
    ) -> Result<(), keyring::Error> {
        let keyring = keyring::Entry::new(service, user)?;

        if let Some(password) = password {
            keyring.set_password(password)?;
        }

        if let Some(secret) = secret {
            keyring.set_password(secret)?;
        }

        Ok(())
    }

    pub fn get_keyring_entry(service: &str, user: &str) -> Result<Entry, keyring::Error> {
        let keyring = keyring::Entry::new(service, user)?;
        Ok(keyring)
    }

    pub fn delete_keyring_entry(service: &str, user: &str) -> Result<(), keyring::Error> {
        let keyring = keyring::Entry::new(service, user)?;
        keyring.delete_credential()?;
        Ok(())
    }
}
