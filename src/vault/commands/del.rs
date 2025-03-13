use super::KeepassVault;
use crate::log_debug;

/// Processes the "del" subcommand by asking the user for confirmation.
/// The user must type the same entry name that was provided at the command line.
pub fn run(keepass_vault: &mut KeepassVault) {
    keepass_vault.set_key().expect("Failed to set key.");
    keepass_vault.open().expect("Failed to open vault file.");
    log_debug!("Vault file: {:?}", keepass_vault);
    println!("🔐 Vault file: {:?}", keepass_vault);
}
