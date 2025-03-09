use crate::log_debug;
use keyring::Entry as KeyringEntry;

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
