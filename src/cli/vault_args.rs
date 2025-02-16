use clap::{Arg, Command};
use std::path::PathBuf;

/// Enum representing different vault subcommands
#[derive(Debug, Clone)]
pub enum VaultArgs {
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
    Lock { vault_file: PathBuf },
    /// Unlock the vault
    Unlock {
        vault_file: PathBuf,
        key_file: Option<PathBuf>,
    },
}

pub fn vault_args() -> Command {
    Command::new("vault")
        .about("Interact with CSH credential vault")
        .arg_required_else_help(true)
        .subcommand_negates_reqs(true)
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
        .arg(
            Arg::new("vault_file")
                .short('v')
                .long("vault-file")
                .value_name("VAULT_FILE")
                .help("Path to the vault file")
                .num_args(1)
                .value_parser(clap::value_parser!(PathBuf))
                .required(true),
        )
        .arg(
            Arg::new("key_file")
                .short('k')
                .long("key")
                .value_name("KEY_FILE")
                .help("Path to a key file (if provided, password prompt is optional)")
                .num_args(1)
                .value_parser(clap::value_parser!(PathBuf)),
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
                .value_name("KEY_FILE")
                .help("Path to a key file (if provided, password prompt is optional)")
                .num_args(1)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn parse_vault_subcommand(matches: &clap::ArgMatches) -> VaultArgs {
    match matches.subcommand() {
        Some(("init", sub_matches)) => VaultArgs::Init {
            vault_name: sub_matches
                .get_one::<String>("vault_name")
                .expect("vault_name is required")
                .clone(),
        },
        Some(("add", sub_matches)) => VaultArgs::Add {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
            use_password: sub_matches.get_flag("password_flag"),
        },
        Some(("del", sub_matches)) => VaultArgs::Delete {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
        },
        Some(("show", sub_matches)) => VaultArgs::Show {
            entry_name: sub_matches
                .get_one::<String>("entry_name")
                .expect("entry_name is required")
                .clone(),
        },
        _ => {
            if matches.get_flag("unlock") {
                VaultArgs::Unlock {
                    vault_file: matches
                        .get_one::<PathBuf>("vault_file")
                        .expect("vault_file is required")
                        .clone(),
                    key_file: matches.get_one::<PathBuf>("key_file").cloned(),
                }
            } else if matches.get_flag("lock") {
                VaultArgs::Lock {
                    vault_file: matches
                        .get_one::<PathBuf>("vault_file")
                        .expect("vault_file is required")
                        .clone(),
                }
            } else {
                panic!("Invalid vault subcommand");
            }
        }
    }
}
