use clap::{Arg, Command};
// use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MainArgs {
    pub debug: bool,
    pub ssh_logging: bool,
    pub ssh_args: Vec<String>,
}

/// Parses command-line arguments using clap.
/// Returns a vector of strings representing the SSH arguments.
pub fn main_args() -> MainArgs {
    let matches = Command::new("csh")
        .version("v0.5.0")
        .author("@karsyboy")
        .about("A Rust-based SSH client with syntax highlighting.")
        .arg_required_else_help(true)
        .subcommand_negates_reqs(true) //set so that sub commands are not required to provide ssh_args
        .propagate_version(true)
        // .subcommand(vault_args())
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
        .arg(Arg::new("ssh_args").help("SSH arguments").num_args(1..).required(true))
        .get_matches();

    // Retrieve remaining SSH arguments
    MainArgs {
        debug: matches.get_flag("debug"),
        ssh_logging: matches.get_flag("log"),
        ssh_args: matches.get_many::<String>("ssh_args").map(|vals| vals.cloned().collect()).unwrap_or_default(),
    }
}