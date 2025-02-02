use crate::enable_debug_mode;
use clap::{Arg, Command};
use std::sync::atomic::{AtomicBool, Ordering};

// Used to enable SSH logging in the logging module
pub static SSH_LOGGING: AtomicBool = AtomicBool::new(false);
fn enable_ssh_logging() {
    SSH_LOGGING.store(true, Ordering::Relaxed);
}

/// Parses command-line arguments using clap.
/// Returns a vector of strings representing the SSH arguments.
pub fn parse_args() -> Vec<String> {
    let matches = Command::new("csh")
        .version("v0.3.3-alpha")
        .author("@karsyboy")
        .about("A Rust-based SSH client with syntax highlighting.")
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

    // Retrieve remaining SSH arguments
    matches
        .get_many::<String>("ssh_args")
        .unwrap()
        .cloned()
        .collect()
}
