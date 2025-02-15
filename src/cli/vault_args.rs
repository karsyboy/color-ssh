use clap::{Arg, Command};
use std::path::PathBuf;

/// Enum representing different vault subcommands
#[derive(Debug, Clone)]
pub enum VaultCommand {
    /// Initialize a new vault
    Init { vault_name: String },
    /// Add a new entry to the vault
    Add {
        entry_name: String,
        key_file: Option<PathBuf>,
        use_password: bool,
    },
    /// Delete an entry from the vault
    Delete { entry_name: String },
    /// Show a vault entry
    Show { entry_name: String },
    /// Lock the vault
    Lock,
    /// Unlock the vault
    Unlock,
}

pub fn vault_args() -> Command {
    Command::new("vault")
        .about("Interact with CSH credential vault")
        .arg_required_else_help(true)
        // Nested subcommands from their own modules:
        .subcommand(add_args())
        .subcommand(del_args())
        .subcommand(init_args())
        .subcommand(list_args())
        // Additional flags that are valid when no subcommand is used:
        .arg(
            Arg::new("unlock")
                .long("unlock")
                .help("Unlock Vault")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("lock")
                .long("lock")
                .help("Lock Vault")
                .action(clap::ArgAction::SetTrue),
        )
}

/// Returns a `clap::Command` for the "add" subcommand,
/// which adds a new vault entry. It requires an entry name and either
/// the `-p` flag (to prompt for a password) or a key file via `--key`.
pub fn add_args() -> Command {
    Command::new("add")
        .about("Add a new vault entry")
        .arg(
            Arg::new("entry_name")
                .help("Name of the vault entry to add")
                .required(true)
                .index(1), // Positional argument for the entry name.
        )
        // The password flag: if provided, prompt the user for the password.
        // This flag is required unless a key file is provided.
        .arg(
            Arg::new("password_flag")
                .short('p')
                .help("Prompt for password to add to the vault entry")
                .action(clap::ArgAction::SetTrue)
                .required_unless_present("key_file"),
        )
        // Optional key file path.
        .arg(
            Arg::new("key_file")
                .short('k')
                .long("key")
                .help("Path to a key file (if provided, password prompt is optional)")
                .num_args(1),
        )
}

/// Returns a `clap::Command` for the "del" subcommand,
/// which deletes a vault entry.
pub fn del_args() -> Command {
    Command::new("del").about("Delete a vault entry").arg(
        Arg::new("entry_name")
            .help("Name of the vault entry to delete")
            .required(true)
            .index(1), // This is a required positional argument.
    )
}

/// Returns a `clap::Command` for the "init" subcommand,
/// which initializes a CSH vault and requires a vault name.
pub fn init_args() -> Command {
    Command::new("init").about("Initialize CSH vault").arg(
        Arg::new("vault_name")
            .help("Name of the vault to initialize")
            .required(true)
            .index(1), // Positional argument at index 1.
    )
}

/// Returns a `clap::Command` for the "show" subcommand,
/// which is used to display a specific vault entry.
pub fn list_args() -> Command {
    Command::new("show").about("Show a vault entry").arg(
        Arg::new("entry_name")
            .help("Name of the vault entry to show")
            .required(true)
            .index(1), // Positional argument at index 1
    )
}

pub fn parse_vault_subcommand(matches: &clap::ArgMatches) -> VaultCommand {
    match matches.subcommand() {
        Some(("init", sub_matches)) => VaultCommand::Init {
            vault_name: sub_matches
                .get_one::<String>("vault_name")
                .expect("vault_name is required")
                .clone(),
        },
        Some(("add", sub_matches)) => VaultCommand::Add {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
            use_password: sub_matches.get_flag("password_flag"),
        },
        Some(("del", sub_matches)) => VaultCommand::Delete {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
        },
        Some(("show", sub_matches)) => VaultCommand::Show {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
        },
        _ => {
            if matches.get_flag("unlock") {
                VaultCommand::Unlock
            } else if matches.get_flag("lock") {
                VaultCommand::Lock
            } else {
                panic!("Invalid vault subcommand");
            }
        }
    }
}
