use clap::{Arg, Command};
use std::{process, sync::atomic::{AtomicBool, Ordering}};

use crate::{enable_debug_mode, vault};

// Used to enable SSH logging in the logging module
pub static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

fn enable_ssh_logging() {
    SSH_LOGGING.store(true, Ordering::Relaxed);
}

/// Parses command-line arguments using clap.
/// Returns a vector of strings representing the SSH arguments.
pub fn parse_args() -> Vec<String> {
    let matches = Command::new("csh")
        .version("v0.4.1")
        .author("@karsyboy")
        .about("A Rust-based SSH client with syntax highlighting.")
        .arg_required_else_help(true)
        .subcommand_negates_reqs(true) //set so that sub commands are not required to provide ssh_args
        .propagate_version(true) 
        .subcommand(vault::vault_args())
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debug mode")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log")
                .short('L')
                .long("log")
                .help("Enable SSH logging")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("ssh_args")
                .help("SSH arguments")
                .num_args(1..)
                .required(true),
        )
        .get_matches();

    // Enable debugging if the flag is set
    if matches.get_flag("debug") {
        enable_debug_mode();
    }

    // Enable SSH logging if the flag is set
    if matches.get_flag("log") {
        enable_ssh_logging();
    }

    // If the vault subcommand is provided, handle it exclusively
    // and then exit without processing any further logic.
    if let Some(("vault", sub_matches)) = matches.subcommand() {
        vault::run(sub_matches);
        process::exit(0); // Exits after processing vault commands
    }

    // Retrieve remaining SSH arguments
    matches
        .get_many::<String>("ssh_args")
        .unwrap()
        .cloned()
        .collect()
        
}
