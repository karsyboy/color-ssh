use super::KeepassVault;
use crate::log_debug;

/// Processes the "add" subcommand by retrieving the entry name,
/// prompting for a password if the -p flag is set, and then handling
/// the key file (if provided).
pub fn run(keepass_vault: &mut KeepassVault) {
    keepass_vault.set_key().expect("Failed to set key.");
    keepass_vault.open().expect("Failed to open vault file.");
    log_debug!("Vault file: {:?}", keepass_vault);
    println!("🔐 Vault file: {:?}", keepass_vault);
}
