use super::{VaultError, keyring};
use crate::log_debug;
use std::{ffi::OsStr, path::PathBuf};

pub fn run(vault_file: &PathBuf) -> Result<(), VaultError> {
    let service = "vault";
    let service_key = "vault_key";
    let vault_file_name = vault_file.file_name().and_then(OsStr::to_str).expect("Failed to get file name");
    println!("🔒 Locking vault...");
    log_debug!("Vault file locked: {}", vault_file.display());
    log_debug!("Vault Hash: {}", vault_file_name);

    keyring::delete_keyring_entry(service, &vault_file_name)?;
    keyring::delete_keyring_entry(service_key, &vault_file_name)?;

    Ok(())
}
