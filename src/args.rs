//! Command-line argument parsing
//!
//! Parses CLI arguments using the clap library and provides structured access
//! to user-provided options.

use clap::{Arg, ArgGroup, Command};
use std::ffi::OsString;

/// Parsed command-line arguments
#[derive(Debug, Clone)]
pub struct MainArgs {
    /// Enable debug logging to file
    pub debug: bool,
    /// Enable SSH session logging to file
    pub ssh_logging: bool,
    /// In test mode, ignore config logging settings and only honor CLI logging flags
    pub test_mode: bool,
    /// Arguments to pass through to the SSH command
    pub ssh_args: Vec<String>,
    /// Argument to pass for configuration profiles
    pub profile: Option<String>,
    /// Whether the SSH command is non-interactive (e.g., -G, -V, -O, -Q, -T)
    pub is_non_interactive: bool,
    /// Launch interactive session manager TUI
    pub interactive: bool,
    /// Add or update a password vault entry
    pub add_pass: Option<String>,
    /// Remove an existing password vault entry.
    pub remove_pass: Option<String>,
    /// Unlock the shared password vault agent.
    pub unlock: bool,
    /// Lock the shared password vault agent.
    pub lock: bool,
    /// Print the current password vault status.
    pub vault_status: bool,
    /// Rotate the master password for the password vault.
    pub set_master_password: bool,
    /// Override the password entry to use for a direct launch.
    pub pass_entry: Option<String>,
    /// Hidden internal mode used to run the background unlock agent.
    pub agent_serve: bool,
}

fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn parse_profile_arg(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if !is_valid_profile_name(trimmed) {
        return Err("invalid profile name: use only letters, numbers, '_' or '-'".to_string());
    }
    Ok(trimmed.to_string())
}

fn is_valid_pass_entry_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_pass_entry_arg(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if !is_valid_pass_entry_name(trimmed) {
        return Err("invalid pass entry name: use only letters, numbers, '.', '_' or '-'".to_string());
    }
    Ok(trimmed.to_string())
}

fn build_cli_command() -> Command {
    Command::new("cossh")
        .version(concat!("v", env!("CARGO_PKG_VERSION")))
        .author("@karsyboy")
        .about("A Rust-based SSH client wrapper with syntax highlighting and logging capabilities")
        .propagate_version(true)
        .group(
            ArgGroup::new("vault_command")
                .args(["add_pass", "remove_pass", "unlock", "lock", "vault_status", "set_master_password"])
                .multiple(false),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debug mode with detailed logging to ~/.color-ssh/logs/cossh.log")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log")
                .short('l')
                .long("log")
                .help("Enable SSH session logging to ~/.color-ssh/logs/ssh_sessions/")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("profile")
                .short('P')
                .long("profile")
                .help("Specify a configuration profile to use")
                .value_parser(clap::builder::ValueParser::new(parse_profile_arg))
                .num_args(1)
                .required(false),
        )
        .arg(
            Arg::new("test")
                .short('t')
                .long("test")
                .help("Ignore config logging settings; only use CLI -d/-l logging flags")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("add_pass")
                .long("add-pass")
                .help("Create or replace a password vault entry interactively")
                .num_args(1)
                .value_name("name")
                .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg))
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("remove_pass")
                .long("remove-pass")
                .help("Remove a password vault entry")
                .num_args(1)
                .value_name("name")
                .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg))
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("unlock")
                .long("unlock")
                .help("Unlock the shared password vault")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("lock")
                .long("lock")
                .help("Lock the shared password vault")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("vault_status")
                .long("vault-status")
                .help("Show shared password vault status")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("set_master_password")
                .long("set-master-password")
                .help("Rotate the password vault master password")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("ssh_args")
                .conflicts_with("profile")
                .conflicts_with("log")
                .conflicts_with("test"),
        )
        .arg(
            Arg::new("pass_entry")
                .long("pass-entry")
                .help("Override the password vault entry used for a direct SSH launch")
                .num_args(1)
                .value_name("name")
                .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg)),
        )
        .arg(Arg::new("ssh_args").help("SSH arguments to forward to the SSH command").num_args(1..))
        .after_help(
            r"
cossh                                              # Launch interactive session manager
cossh --add-pass office_fw                         # Create/update password vault entry 'office_fw'
cossh --remove-pass office_fw                      # Remove password vault entry 'office_fw'
cossh --unlock                                     # Unlock the shared password vault
cossh --lock                                       # Lock the shared password vault
cossh --vault-status                               # Show password vault status
cossh --set-master-password                        # Rotate the vault master password
cossh -d                                           # Launch interactive session manager with debug enabled
cossh -d user@example.com                          # Debug mode enabled
cossh --pass-entry office_fw user@example.com      # Override the password entry for this launch
cossh -l user@example.com                          # SSH logging enabled
cossh -l -P network user@firewall.example.com      # Use 'network' config profile
cossh -l user@host -p 2222                         # Both modes with SSH args
cossh -tld -P network localhost                    # Test mode: force logging from CLI flags only
cossh user@host -G                                 # Non-interactive command
",
        )
}

fn detect_non_interactive_ssh_args(ssh_args: &[String]) -> bool {
    ssh_args.iter().any(|arg| matches!(arg.as_str(), "-G" | "-V" | "-O" | "-Q"))
}

fn is_agent_serve_command(raw_args: &[OsString]) -> bool {
    raw_args.len() == 3 && raw_args.get(1).and_then(|arg| arg.to_str()) == Some("agent") && raw_args.get(2).and_then(|arg| arg.to_str()) == Some("--serve")
}

fn parse_main_args_from<I, T>(cmd: &Command, raw_args: I) -> MainArgs
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw_args: Vec<OsString> = raw_args.into_iter().map(Into::into).collect();

    if is_agent_serve_command(&raw_args) {
        return MainArgs {
            debug: false,
            ssh_logging: false,
            test_mode: false,
            ssh_args: Vec::new(),
            profile: None,
            is_non_interactive: false,
            interactive: false,
            add_pass: None,
            remove_pass: None,
            unlock: false,
            lock: false,
            vault_status: false,
            set_master_password: false,
            pass_entry: None,
            agent_serve: true,
        };
    }

    let matches = cmd.clone().get_matches_from(raw_args.clone());

    // Retrieve SSH arguments to forward.
    let ssh_args: Vec<String> = matches.get_many::<String>("ssh_args").map(|vals| vals.cloned().collect()).unwrap_or_default();
    let debug = matches.get_flag("debug");
    let ssh_logging = matches.get_flag("log");
    let test_mode = matches.get_flag("test");
    let profile = matches.get_one::<String>("profile").cloned().filter(|profile_name| !profile_name.is_empty());
    let add_pass = matches.get_one::<String>("add_pass").cloned().filter(|value| !value.is_empty());
    let remove_pass = matches.get_one::<String>("remove_pass").cloned().filter(|value| !value.is_empty());
    let unlock = matches.get_flag("unlock");
    let lock = matches.get_flag("lock");
    let vault_status = matches.get_flag("vault_status");
    let set_master_password = matches.get_flag("set_master_password");
    let pass_entry = matches.get_one::<String>("pass_entry").cloned().filter(|value| !value.is_empty());
    let no_user_args = raw_args.len() <= 1;
    let debug_only = debug
        && !ssh_logging
        && profile.is_none()
        && ssh_args.is_empty()
        && add_pass.is_none()
        && remove_pass.is_none()
        && !unlock
        && !lock
        && !vault_status
        && !set_master_password
        && pass_entry.is_none();
    let interactive = no_user_args || debug_only;

    MainArgs {
        debug,
        ssh_logging,
        test_mode,
        interactive,
        profile,
        is_non_interactive: detect_non_interactive_ssh_args(&ssh_args),
        ssh_args,
        add_pass,
        remove_pass,
        unlock,
        lock,
        vault_status,
        set_master_password,
        pass_entry,
        agent_serve: false,
    }
}

/// Parses command-line arguments using clap.
///
/// # Arguments Supported
/// - `-d, --debug` - Enable debug mode with detailed logging
/// - `-l, --log` - Enable SSH session logging
/// - `-t, --test` - Ignore config logging settings and use only CLI `-d/-l` logging flags
/// - `--add-pass <name>` - Create or update a password vault entry interactively
/// - `--remove-pass <name>` - Remove a password vault entry
/// - `--unlock` - Unlock the shared password vault
/// - `--lock` - Lock the shared password vault
/// - `--vault-status` - Show shared password vault status
/// - `--set-master-password` - Rotate the master password
/// - `--pass-entry <name>` - Override the password entry for a direct launch
/// - `ssh_args` - All remaining arguments are passed to SSH
///
/// # Examples
/// ```text
/// cossh                              # Launch interactive session manager (default when no args)
/// cossh -d                           # Launch interactive session manager with debug enabled
/// cossh --add-pass office_fw         # Create/update password vault entry
/// cossh --unlock                     # Unlock the shared password vault
/// cossh -d user@example.com          # Debug mode enabled
/// cossh --pass-entry office_fw user@example.com
/// cossh -l user@example.com          # SSH logging enabled
/// cossh -tld -P network localhost    # Test mode with CLI-controlled logging
/// cossh -d -l user@example.com -p 22 # Both modes with SSH args
/// cossh -- -G user@example.com       # Non-interactive command (config dump).
/// ```
///
/// # Returns
/// A `MainArgs` struct containing all parsed arguments
pub fn main_args() -> MainArgs {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, std::env::args_os());

    if parsed.agent_serve {
        return parsed;
    }

    if parsed.add_pass.is_none()
        && parsed.remove_pass.is_none()
        && !parsed.unlock
        && !parsed.lock
        && !parsed.vault_status
        && !parsed.set_master_password
        && !parsed.interactive
        && parsed.ssh_args.is_empty()
    {
        let mut help_cmd = cmd;
        let _ = help_cmd.print_long_help();
        println!();
        std::process::exit(2);
    }

    parsed
}

#[cfg(test)]
#[path = "test/args.rs"]
mod tests;
