//! Command-line argument parsing
//!
//! Parses CLI arguments using the clap library and provides structured access
//! to user-provided options.

use clap::{Arg, Command};

/// Parsed command-line arguments
#[derive(Debug, Clone)]
pub struct MainArgs {
    /// Enable debug logging to file
    pub debug: bool,
    /// Enable SSH session logging to file
    pub ssh_logging: bool,
    /// Arguments to pass through to the SSH command
    pub ssh_args: Vec<String>,
    /// Whether the SSH command is non-interactive (e.g., -G, -V, -O, -Q, -T)
    pub is_non_interactive: bool,
}

/// Parses command-line arguments using clap.
///
/// # Arguments Supported
/// - `-d, --debug` - Enable debug mode with detailed logging
/// - `-L, --log` - Enable SSH session logging
/// - `ssh_args` - All remaining arguments are passed to SSH
///
/// # Examples
/// ```text
/// csh -d user@example.com          # Debug mode enabled
/// csh -L user@example.com          # SSH logging enabled
/// csh -d -L user@example.com -p 22 # Both modes with SSH args
/// csh user@example.com -G          # Non-interactive command (config dump)
/// ```
///
/// # Returns
/// A MainArgs struct containing all parsed arguments
pub fn main_args() -> MainArgs {
    let matches = Command::new("csh")
        .version("v0.5.0")
        .author("@karsyboy")
        .about("A Rust-based SSH client wrapper with syntax highlighting and logging capabilities")
        .arg_required_else_help(true)
        .propagate_version(true)
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debug mode with detailed logging to ~/.csh/logs/csh.log")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log")
                .short('l')
                .long("log")
                .help("Enable SSH session logging to ~/.csh/logs/ssh_sessions/")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("ssh_args")
                .help("SSH arguments to forward to the SSH command")
                .num_args(1..)
                .required(true)
        )
        .get_matches();

    // Retrieve SSH arguments to forward
    let ssh_args: Vec<String> = matches
        .get_many::<String>("ssh_args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    // Detect non-interactive SSH commands that don't need highlighting
    // These commands typically output configuration or version info
    let is_non_interactive = ssh_args.iter().any(|arg| {
        matches!(arg.as_str(), "-G" | "-V" | "-O" | "-Q" | "-T")
    });

    MainArgs {
        debug: matches.get_flag("debug"),
        ssh_logging: matches.get_flag("log"),
        ssh_args,
        is_non_interactive,
    }
}