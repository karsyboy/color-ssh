use clap::{Arg, Command};
use std::path::PathBuf;

/// Enum representing different vault subcommands
#[derive(Debug, Clone)]
pub enum VaultArgs {
    /// Initialize a new vault
    Init {
        vault_file: PathBuf,
        key_file: Option<PathBuf>,
    },
    /// Add a new entry to the vault
    Add {
        vault_file: Option<PathBuf>,
        key_file: Option<PathBuf>,
    },
    /// Delete an entry from the vault
    Delete {
        vault_file: Option<PathBuf>,
        key_file: Option<PathBuf>,
    },
    /// Show a vault entry
    Show {
        vault_file: Option<PathBuf>,
        key_file: Option<PathBuf>,
    },
    /// Lock the vault
    Lock { vault_file: Option<PathBuf> },
    /// Unlock the vault
    Unlock {
        vault_file: Option<PathBuf>,
        key_file: Option<PathBuf>,
    },
}

pub fn vault_args() -> Command {
    Command::new("vault")
        .about("Interact with CSH credential vault")
        .arg_required_else_help(true)
        .subcommand_negates_reqs(true)
        // Nested subcommands from their own modules:
        .subcommand(init_args())
        .subcommand(show_args())
        .subcommand(add_args())
        .subcommand(del_args())
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
                .global(true)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn init_args() -> Command {
    Command::new("init")
        .about("Initialize CSH vault")
        .arg(
            Arg::new("vault_file")
                .short('v')
                .long("vault-file")
                .value_name("VAULT_FILE")
                .help("Path to the vault file")
                .num_args(1)
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn show_args() -> Command {
    Command::new("show")
        .about("Show a vault entry")
        .arg(
            Arg::new("vault_file")
                .short('v')
                .long("vault-file")
                .value_name("VAULT_FILE")
                .help("Path to the vault file")
                .num_args(1)
                .global(true)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn add_args() -> Command {
    Command::new("add")
        .about("Add a new vault entry")
        .arg(
            Arg::new("vault_file")
                .short('v')
                .long("vault-file")
                .value_name("VAULT_FILE")
                .help("Path to the vault file")
                .num_args(1)
                .global(true)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn del_args() -> Command {
    Command::new("del")
        .about("Delete a vault entry")
        .arg(
            Arg::new("vault_file")
                .short('v')
                .long("vault-file")
                .value_name("VAULT_FILE")
                .help("Path to the vault file")
                .num_args(1)
                .global(true)
                .value_parser(clap::value_parser!(PathBuf)),
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

pub fn parse_vault_subcommand(matches: &clap::ArgMatches) -> VaultArgs {
    match matches.subcommand() {
        Some(("init", sub_matches)) => VaultArgs::Init {
            vault_file: sub_matches
                .get_one::<PathBuf>("vault_file")
                .cloned()
                .unwrap(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
        },
        Some(("show", sub_matches)) => VaultArgs::Show {
            vault_file: sub_matches.get_one::<PathBuf>("vault_file").cloned(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
        },
        Some(("add", sub_matches)) => VaultArgs::Add {
            vault_file: sub_matches.get_one::<PathBuf>("vault_file").cloned(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
        },
        Some(("del", sub_matches)) => VaultArgs::Delete {
            vault_file: sub_matches.get_one::<PathBuf>("vault_file").cloned(),
            key_file: sub_matches.get_one::<PathBuf>("key_file").cloned(),
        },
        _ => {
            if matches.get_flag("unlock") {
                VaultArgs::Unlock {
                    vault_file: matches.get_one::<PathBuf>("vault_file").cloned(),
                    key_file: matches.get_one::<PathBuf>("key_file").cloned(),
                }
            } else if matches.get_flag("lock") {
                VaultArgs::Lock {
                    vault_file: matches.get_one::<PathBuf>("vault_file").cloned(),
                }
            } else {
                panic!("Invalid vault subcommand");
            }
        }
    }
}
