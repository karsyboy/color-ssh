/*
TODO:
    - for linux something like gnome-keyring may be required now.
*/

use keyring::Entry as KeyringEntry;
use rpassword::prompt_password;
use secrecy::{ExposeSecret, SecretBox};
use std::path::PathBuf;
use std::io::{self, Write};

use keepass::{
    db::{Entry as KeepassEntry, Group, Node},
    error::DatabaseOpenError,
    Database, DatabaseKey,
};

use super::VaultError;
use crate::cli::VaultArgs;
use crate::config::CONFIG;
use crate::log_debug;
use crate::vault::commands::{lock_vault, unlock_vault, init_vault};

pub struct VaultManager {
    vault_hash: String,
    vault_file: PathBuf,
    vault_key: Option<PathBuf>,
    vault_password: SecretBox<String>,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            vault_hash: String::new(),
            vault_file: PathBuf::new(),
            vault_key: Some(PathBuf::new()),
            vault_password: SecretBox::new(Box::new(String::new())),
        }
    }

    pub fn get_vault_values(&self) -> (String, PathBuf, Option<PathBuf>, &SecretBox<String>) {
        (
            self.vault_hash.clone(),
            self.vault_file.clone(),
            self.vault_key.clone(),
            &self.vault_password,
        )
    }
    pub fn set_vault_hash(&mut self, hash: String) {
        log_debug!("Setting vault hash: {}", hash);
        self.vault_hash = hash;
    }

    pub fn get_vault_hash(&self) -> &String {
        &self.vault_hash
    }

    pub fn set_vault_file(&mut self, path: PathBuf) {
        log_debug!("Setting vault file: {}", path.display());
        self.vault_file = path;
    }

    pub fn get_vault_path(&self) -> &PathBuf {
        &self.vault_file
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
            VaultArgs::Init { vault_file, key_file } => {
                init_vault(&vault_file, key_file.as_ref()).unwrap_or_else(|err| {
                    println!("Failed to initialize vault: {}", err);
                });
            }
            VaultArgs::Show { entry_name } => {
                println!("Showing entry: {}", entry_name);
                // show::run(entry_name);
            }
            VaultArgs::Lock { vault_file } => {
                if let Some(vault_file) = vault_file {
                    println!("Locking vault: {}", vault_file.display());
                    let _ = lock_vault(&vault_file);
                } else {
                    println!("Loading vault file from config.");
                    let vault_file = CONFIG.read().unwrap().settings.vault_path.clone();
                    if let Some(vault_file) = vault_file {
                        println!("Locking vault: {}", vault_file.display());
                        let _ = lock_vault(&vault_file);
                    } else {
                        println!("No vault file found in config.");
                    }
                }
            }
            VaultArgs::Unlock {
                vault_file,
                key_file,
            } => {
                if let Some(vault_file) = vault_file {
                    CONFIG.write().unwrap().settings.vault_path = Some(vault_file.clone());
                }
                if let Some(key_file) = key_file {
                    CONFIG.write().unwrap().settings.vault_key = Some(key_file.clone());
                }

                let vault_file = CONFIG.read().unwrap().settings.vault_path.clone();
                let key_file = CONFIG.read().unwrap().settings.vault_key.clone();
                if let Some(vault_file) = vault_file {
                    println!("Unlocking vault: {}", vault_file.display());
                    let _ = unlock_vault(&vault_file, key_file.as_ref());
                } else {
                    println!("No vault file found.");
                }
            }
        }
    }

    pub fn get_password() -> Result<SecretBox<String>, VaultError> {
        let password = SecretBox::new(Box::new(prompt_password("Enter your password: ")?));
        let verified_password = SecretBox::new(Box::new(prompt_password("Verify your password: ")?));
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
            print!("{} [Y/n]: ", question);
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
        let mut vault_name = String::new();
        print!("Enter vault name: ");
        io::stdout().flush().unwrap();
        io::stdin().read_line(&mut vault_name).unwrap();
        let vault_name = vault_name.trim().to_string();

        if vault_name.is_empty() {
            return Err(VaultError::EmptyPassword);
        }

        Ok(vault_name)
    }

    pub fn create_keyring_entry(
        service: &str,
        user: &str,
        password: Option<&str>,
        secret: Option<&str>,
    ) -> Result<(), keyring::Error> {
        let keyring = KeyringEntry::new(service, user)?;

        if let Some(password) = password {
            keyring.set_password(password)?;
        }

        if let Some(secret) = secret {
            keyring.set_password(secret)?;
        }

        Ok(())
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

    pub fn vault_key_exists(key_file: &Option<&PathBuf>) -> bool {
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

    pub fn new_keepass_db() -> Result<Database, DatabaseOpenError> {
        let db = Database::new(Default::default());
        Ok(db)
    }

    pub fn new_vault_key(key_file: Option<&PathBuf>, password: SecretBox<String>) -> Result<DatabaseKey, VaultError> {
        let password = if password.expose_secret().is_empty() {
            None
        } else {
            Some(password.expose_secret().clone())
        };
        let key_file = if let Some(key_file) = key_file {
            Some(key_file.to_path_buf())
        } else {
            None
        };
        let vault_key = DatabaseKey::new();

        let vault_key = match password {
            Some(password) => vault_key.with_password(password.as_str()),
            None => vault_key,
        };

        if key_file.is_none() {
            return Ok(vault_key);
        } else {
            let mut file = std::fs::File::open(key_file.clone().unwrap())?;

            let vault_key = match key_file {
                Some(_) => vault_key.with_keyfile(&mut file)?,
                None => vault_key,
            };
            Ok(vault_key)
        }
    }

    pub fn create_key_file(key_path: Option<&PathBuf>) -> Result<&PathBuf, VaultError> {
        if key_path.is_some() {
            let mut key_file = std::fs::File::create(&key_path.unwrap())?;

            let random_bytes = rand::random::<[u8; 32]>();
            key_file.write_all(&random_bytes)?;
        } else {
            let home_dir = dirs::home_dir().expect("Failed to get home directory.\r");
            let csh_dir = home_dir.join(".csh");
            let vault_dir = csh_dir.join("vault");
            let key_path = vault_dir.join("vault.key");

            if !vault_dir.exists() {
                std::fs::create_dir_all(&vault_dir)?;
            }

            let mut key_file = std::fs::File::create(&key_path)?;
            
            // write random bytes to the file
            let random_bytes = rand::random::<[u8; 32]>();
            key_file.write_all(&random_bytes)?;
        }
        
        Ok(key_path.unwrap())
    }

}
