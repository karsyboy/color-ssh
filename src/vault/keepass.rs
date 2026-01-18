use super::VaultError;
use keepass::{
    Database, DatabaseKey,
    db::{Entry, Group, Node},
    error::DatabaseOpenError,
};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretBox};
use std::{fs::File, io::BufWriter, io::Write, path::PathBuf};

#[derive(Debug)]
pub struct KeepassVault {
    pub db_file: PathBuf,
    pub password: SecretBox<String>,
    pub key_file: Option<PathBuf>,
    pub db: Database,
    pub key: DatabaseKey,
}

impl KeepassVault {
    // this should always be ran first
    pub fn new(db_file: PathBuf, password: SecretBox<String>, key_file: Option<PathBuf>) -> KeepassVault {
        KeepassVault {
            db_file,
            password,
            key_file,
            db: Database::new(Default::default()),
            key: DatabaseKey::new(),
        }
    }

    pub fn create_key_file(key_file: PathBuf) -> Result<PathBuf, VaultError> {
        if let Some(parent) = key_file.parent() {
            std::fs::create_dir_all(parent).map_err(|err| VaultError::from(err))?;
        }

        let mut key = [0u8; 128];
        rand::rng().fill_bytes(&mut key);

        // Create the key file
        let file = File::create(&key_file).map_err(|err| VaultError::from(err))?;

        let mut writer = BufWriter::new(file);
        writer.write_all(&key).map_err(|err| VaultError::from(err))?;

        Ok(key_file)
    }

    pub fn create(&mut self) -> Result<(), VaultError> {
        let mut db_file = File::create(&self.db_file).map_err(|err| VaultError::from(err))?;
        self.db.save(&mut db_file, self.key.clone()).map_err(|err| VaultError::from(err))?;
        Ok(())
    }

    pub fn open(&mut self) -> Result<(), DatabaseOpenError> {
        let mut db_file = File::open(&self.db_file)?;
        let db = Database::open(&mut db_file, self.key.clone())?;
        self.db = db;
        Ok(())
    }

    pub fn save(&self) -> Result<(), VaultError> {
        let mut db_file = File::open(&self.db_file).map_err(|err| VaultError::from(err))?;
        self.db.save(&mut db_file, self.key.clone()).map_err(|err| VaultError::from(err))?;
        Ok(())
    }

    pub fn set_key(&mut self) -> Result<(), VaultError> {
        let password = self.password.expose_secret().clone();

        let key_file = if let Some(key_file) = &self.key_file { Some(key_file.clone()) } else { None };

        let key = DatabaseKey::new();

        let key = key.with_password(password.as_str());

        let key = match key_file {
            Some(key_file) => {
                let mut file = File::open(key_file).map_err(|err| VaultError::from(err))?;
                key.with_keyfile(&mut file).unwrap()
            }
            None => key,
        };

        self.key = key;
        Ok(())
    }

    pub fn get_all_entries(&self) -> Vec<WrappedEntry<'_>> {
            get_entries(&self.db.root, "")
    }
    
}

fn get_entries(group: &Group, path: impl ToString) -> Vec<WrappedEntry<'_>> {
    let mut entries = Vec::with_capacity(
        group
            .children
            .iter()
            .filter(|v| match v {
                Node::Entry(_) => true,
                Node::Group(_) => false,
            })
            .count(),
    );
    group.children.iter().for_each(|v| match v {
        Node::Entry(entry) => entries.push(WrappedEntry {
            path: format!("{}/{}", path.to_string(), group.name),
            entry,
        }),
        Node::Group(child) => {
            if !child.children.is_empty() {
                entries.extend(get_entries(
                    child,
                    format!("{}/{}", path.to_string(), group.name),
                ))
            }
        }
    });
    entries
}

#[derive(Debug, Clone)]
pub struct WrappedEntry<'a> {
    pub path: String,
    pub entry: &'a Entry,
}

pub trait EntryPath {
    fn entry_path(&self) -> String;
    fn get_entry(&self) -> &Entry;
    fn get_title(&self) -> String;
}

impl EntryPath for WrappedEntry<'_> {
    fn entry_path(&self) -> String {
        format!(
            "{}/{}",
            self.path,
            self.entry.get_title().unwrap_or_default().to_owned(),
        )
    }

    fn get_entry(&self) -> &Entry {
        self.entry
    }

    fn get_title(&self) -> String {
        self.entry.get_title().unwrap_or_default().to_owned()
    }

}
