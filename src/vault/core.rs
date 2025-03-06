/*
TODO:
    - for linux something like gnome-keyring may be required now.
*/

use keyring::Entry as KeyringEntry;
use rpassword::prompt_password;
use secrecy::{ExposeSecret, SecretBox};
use std::io::{self, Write};
use std::path::PathBuf;

use super::VaultError;
use crate::args::VaultArgs;
use crate::config::CONFIG;
use crate::log_debug;
use crate::vault::commands::{
    add_entry, del_entry, init_vault, lock_vault, show_vault, unlock_vault,
};

pub struct VaultManager {
    vault_file_name: String,
    vault_path: PathBuf,
    vault_key: Option<PathBuf>,
    vault_password: SecretBox<String>,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            vault_file_name: String::new(),
            vault_path: PathBuf::new(),
            vault_key: Some(PathBuf::new()),
            vault_password: SecretBox::new(Box::new(String::new())),
        }
    }

    pub fn get_vault_values(&self) -> (String, PathBuf, Option<PathBuf>, &SecretBox<String>) {
        (
            self.vault_file_name.clone(),
            self.vault_path.clone(),
            self.vault_key.clone(),
            &self.vault_password,
        )
    }
    pub fn set_vault_file_name(&mut self, name: &str) {
        log_debug!("Setting vault hash: {}", name);
        self.vault_file_name = name.to_string();
    }

    pub fn get_vault_file_name(&self) -> &String {
        &self.vault_file_name
    }

    pub fn set_vault_path(&mut self, path: PathBuf) {
        log_debug!("Setting vault file: {}", path.display());
        self.vault_path = path;
    }

    pub fn get_vault_path(&self) -> &PathBuf {
        &self.vault_path
    }

    pub fn set_vault_key(&mut self, key: PathBuf) {
        log_debug!("Setting vault key: {}", key.display());
        self.vault_key = Some(key);
    }

    pub fn get_vault_key(&self) -> Option<&PathBuf> {
        self.vault_key.as_ref()
    }

    pub fn set_vault_password(&mut self, password: SecretBox<String>) {
        self.vault_password = password;
    }

    pub fn get_vault_password(&self) -> &SecretBox<String> {
        &self.vault_password
    }

    pub fn get_password() -> Result<SecretBox<String>, VaultError> {
        let password = SecretBox::new(Box::new(prompt_password("Enter your password: ")?));
        let verified_password =
            SecretBox::new(Box::new(prompt_password("Verify your password: ")?));
        if password.expose_secret() != verified_password.expose_secret() {
            return Err(VaultError::InvalidPassword);
        }

        if password.expose_secret().is_empty() {
            return Err(VaultError::EmptyPassword);
        }

        Ok(password)
    }

    pub fn ask_yes_no(question: &str, default: bool) -> bool {
        let mut answer = String::new();
        loop {
            if default {
                print!("{} [Y/n]: ", question);
            } else {
                print!("{} [y/N]: ", question);
            }
            io::stdout().flush().unwrap();

            answer.clear();
            io::stdin().read_line(&mut answer).unwrap();

            let input = answer.trim().to_lowercase();
            if input.is_empty() {
                return default; // Default to "yes" if no input is given
            } else if input == "y" || input == "n" {
                return input == "y";
            }
        }
    }

    pub fn get_vault_name() -> Result<String, VaultError> {
        let mut vault_file_name = String::new();
        print!("Enter vault name: ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut vault_file_name).unwrap();
        let vault_file_name = vault_file_name.trim().to_string();

        if vault_file_name.is_empty() {
            return Err(VaultError::EmptyPassword);
        }

        Ok(vault_file_name)
    }

    pub fn get_keyring_entry(service: &str, user: &str) -> Result<KeyringEntry, keyring::Error> {
        let keyring = KeyringEntry::new(service, user)?;
        Ok(keyring)
    }

    pub fn delete_keyring_entry(service: &str, user: &str) -> Result<(), keyring::Error> {
        let keyring = KeyringEntry::new(service, user)?;
        log_debug!("Deleting keyring entry: {:?}", keyring);

        keyring.delete_credential()?;
        Ok(())
    }

    pub fn vault_exists(vault_file: &PathBuf) -> bool {
        if vault_file.exists() {
            println!("Vault file exists: {}", vault_file.display());
            true
        } else {
            println!("Vault file does not exist: {}", vault_file.display());
            false
        }
    }

    pub fn vault_key_exists(key_file: &Option<PathBuf>) -> bool {
        if let Some(key_file) = key_file {
            if key_file.exists() {
                println!("Key file exists: {}", key_file.display());
                return true;
            } else {
                println!("Key file does not exist: {}", key_file.display());
            }
        }
        false
    }

    pub fn unlock_vault(
        vault_file: Option<PathBuf>,
        key_file: Option<PathBuf>,
    ) -> Result<VaultManager, VaultError> {
        if let Some(vault_file) = vault_file {
            CONFIG.write().unwrap().settings.vault_path = Some(vault_file.clone());
        }
        if let Some(key_file) = key_file {
            CONFIG.write().unwrap().settings.vault_key = Some(key_file.clone());
        }

        let vault_file = CONFIG.read().unwrap().settings.vault_path.clone();
        let key_file = CONFIG.read().unwrap().settings.vault_key.clone();

        if let Some(vault_file) = vault_file {
            let vault_manager = unlock_vault(&vault_file, key_file.as_ref());
            vault_manager
        } else {
            println!("No vault file found.");
            Err(VaultError::NoVaultFile)
        }
    }
}

pub fn vault_handler(vault_commands: VaultArgs) {
    match vault_commands {
        VaultArgs::Init {
            vault_file,
            key_file,
        } => {
            init_vault(vault_file, key_file).unwrap_or_else(|err| {
                println!("Failed to initialize vault: {}", err);
            });
        }
        VaultArgs::Show {
            vault_file,
            key_file,
        } => {
            let vault_manager = VaultManager::unlock_vault(vault_file, key_file)
                .expect("❌ Failed to unlock vault");
            show_vault(&vault_manager);
        }
        VaultArgs::Add {
            vault_file,
            key_file,
        } => {
            let vault_manager = VaultManager::unlock_vault(vault_file, key_file)
                .expect("❌ Failed to unlock vault");
            add_entry(&vault_manager);
        }
        VaultArgs::Delete {
            vault_file,
            key_file,
        } => {
            let vault_manager = VaultManager::unlock_vault(vault_file, key_file)
                .expect("❌ Failed to unlock vault");
            del_entry(&vault_manager);
        }
        VaultArgs::Lock { vault_file } => {
            if let Some(vault_file) = vault_file {
                let _ = lock_vault(&vault_file);
            } else {
                let vault_file = CONFIG.read().unwrap().settings.vault_path.clone();
                if let Some(vault_file) = vault_file {
                    let _ = lock_vault(&vault_file);
                } else {
                    println!("❌ No vault file found in config.");
                }
            }
        }
        VaultArgs::Unlock {
            vault_file,
            key_file,
        } => {
            VaultManager::unlock_vault(vault_file, key_file).expect("❌ Failed to unlock vault");
        }
    }
}
