use super::KeepassVault;
use crate::log_debug;
use keepass::db::NodeRef;
//     db::{Database, NodeRef},
//     error::{DatabaseKeyError, DatabaseOpenError},
//     DatabaseKey,
// };

/// Processes the "show" subcommand.
/// It retrieves the vault entry name from the command-line arguments and prints it.
pub fn run(keepass_vault: &KeepassVault) {
    log_debug!("Vault file: {:?}", keepass_vault);
    println!("🔐 Vault file: {:?}", keepass_vault);

    let keepass_db = keepass_vault.db.clone();

    if let Some(NodeRef::Entry(entry)) = keepass_db.root.get(&["dick"]) {
        println!("Title: {:?}", entry.get_password())

    }
}
