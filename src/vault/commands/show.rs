use super::VaultManager;
use secrecy::ExposeSecret;

/// Processes the "show" subcommand.
/// It retrieves the vault entry name from the command-line arguments and prints it.
pub fn run(vault_manager: &VaultManager) {
    let vault_file_name = vault_manager.get_vault_file_name();
    let vault_file = vault_manager.get_vault_path();
    let vault_key = vault_manager.get_vault_key();
    let vault_password = vault_manager.get_vault_password();

    println!("Vault file: {}", vault_file.display());
    println!("Vault file name: {}", vault_file_name);
    println!("Vault key: {:?}", vault_key);
    println!("Vault password: {}", vault_password.expose_secret());
}
