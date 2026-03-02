//! Command-line argument parsing
//!
//! Parses CLI arguments using the clap library and provides structured access
//! to user-provided options.

use clap::{Arg, Command};
use std::ffi::OsString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultCommand {
    Init,
    AddPass(String),
    RemovePass(String),
    List,
    Unlock,
    Lock,
    Status,
    SetMasterPassword,
}

/// Parsed command-line arguments
#[derive(Debug, Clone)]
pub struct MainArgs {
    /// Debug verbosity requested on the CLI (`-d` safe, `-dd` raw).
    pub debug_count: u8,
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
    /// Vault management subcommand.
    pub vault_command: Option<VaultCommand>,
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
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Enable debug logging to ~/.color-ssh/logs/cossh.log; repeat (-dd) for raw terminal and argument tracing")
                .action(clap::ArgAction::Count),
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
            Arg::new("pass_entry")
                .long("pass-entry")
                .help("Override the password vault entry used for a direct SSH launch")
                .num_args(1)
                .value_name("name")
                .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg)),
        )
        .arg(Arg::new("ssh_args").help("SSH arguments to forward to the SSH command").num_args(1..))
        .subcommand(
            Command::new("vault")
                .about("Manage the password vault")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(Command::new("init").about("Initialize the password vault"))
                .subcommand(
                    Command::new("add").about("Create or replace a password vault entry interactively").arg(
                        Arg::new("name")
                            .help("Password entry name")
                            .required(true)
                            .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg)),
                    ),
                )
                .subcommand(
                    Command::new("remove").about("Remove a password vault entry").arg(
                        Arg::new("name")
                            .help("Password entry name")
                            .required(true)
                            .value_parser(clap::builder::ValueParser::new(parse_pass_entry_arg)),
                    ),
                )
                .subcommand(Command::new("list").about("List password vault entries"))
                .subcommand(Command::new("unlock").about("Unlock the shared password vault"))
                .subcommand(Command::new("lock").about("Lock the shared password vault"))
                .subcommand(Command::new("status").about("Show shared password vault status"))
                .subcommand(Command::new("set-master-password").about("Create or rotate the password vault master password")),
        )
        .subcommand(
            Command::new("agent")
                .hide(true)
                .subcommand_required(false)
                .arg_required_else_help(false)
                .arg(Arg::new("serve").long("serve").hide(true).action(clap::ArgAction::SetTrue)),
        )
        .after_help(
            r"
cossh                                              # Launch interactive session manager
cossh -d                                           # Launch interactive session manager with safe debug enabled
cossh -dd user@example.com                         # Raw debug enabled (may log terminal content and secrets)
cossh -d user@example.com                          # Safe debug enabled
cossh --pass-entry office_fw user@example.com      # Override the password entry for this launch
cossh -l user@example.com                          # SSH logging enabled
cossh -l -P network user@firewall.example.com      # Use 'network' config profile
cossh -l user@host -p 2222                         # Both modes with SSH args
cossh user@host -G                                 # Non-interactive command
",
        )
}

fn detect_non_interactive_ssh_args(ssh_args: &[String]) -> bool {
    ssh_args.iter().any(|arg| matches!(arg.as_str(), "-G" | "-V" | "-O" | "-Q"))
}

fn parse_vault_command(matches: &clap::ArgMatches) -> Option<VaultCommand> {
    let ("vault", vault_matches) = matches.subcommand()? else {
        return None;
    };

    match vault_matches.subcommand() {
        Some(("init", _)) => Some(VaultCommand::Init),
        Some(("add", add_pass_matches)) => add_pass_matches.get_one::<String>("name").cloned().map(VaultCommand::AddPass),
        Some(("remove", remove_pass_matches)) => remove_pass_matches.get_one::<String>("name").cloned().map(VaultCommand::RemovePass),
        Some(("list", _)) => Some(VaultCommand::List),
        Some(("unlock", _)) => Some(VaultCommand::Unlock),
        Some(("lock", _)) => Some(VaultCommand::Lock),
        Some(("status", _)) => Some(VaultCommand::Status),
        Some(("set-master-password", _)) => Some(VaultCommand::SetMasterPassword),
        _ => None,
    }
}

fn parse_main_args_from<I, T>(cmd: &Command, raw_args: I) -> MainArgs
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw_args: Vec<OsString> = raw_args.into_iter().map(Into::into).collect();

    let matches = cmd.clone().get_matches_from(raw_args.clone());

    // Retrieve SSH arguments to forward.
    let ssh_args: Vec<String> = matches.get_many::<String>("ssh_args").map(|vals| vals.cloned().collect()).unwrap_or_default();
    let debug_count = matches.get_count("debug");
    let ssh_logging = matches.get_flag("log");
    let test_mode = matches.get_flag("test");
    let profile = matches.get_one::<String>("profile").cloned().filter(|profile_name| !profile_name.is_empty());
    let vault_command = parse_vault_command(&matches);
    let pass_entry = matches.get_one::<String>("pass_entry").cloned().filter(|value| !value.is_empty());
    let agent_serve = matches
        .subcommand()
        .is_some_and(|(name, sub_matches)| name == "agent" && sub_matches.get_flag("serve"));
    let no_user_args = raw_args.len() <= 1;
    let debug_only = debug_count > 0 && !ssh_logging && profile.is_none() && ssh_args.is_empty() && vault_command.is_none() && pass_entry.is_none();
    let interactive = (no_user_args || debug_only) && !agent_serve;

    MainArgs {
        debug_count,
        ssh_logging,
        test_mode,
        interactive,
        profile,
        is_non_interactive: detect_non_interactive_ssh_args(&ssh_args),
        ssh_args,
        vault_command,
        pass_entry,
        agent_serve,
    }
}

/// Parses command-line arguments using clap.
///
/// # Arguments Supported
/// - `-d, --debug` - Enable safe debug mode with detailed metadata logging
/// - `-dd` - Enable raw-content debug tracing for troubleshooting
/// - `-l, --log` - Enable SSH session logging
/// - `-t, --test` - Ignore config logging settings and use only CLI `-d/-l` logging flags
/// - `vault init` - Initialize the password vault
/// - `vault add <name>` - Create or update a password vault entry interactively
/// - `vault remove <name>` - Remove a password vault entry
/// - `vault list` - List current password vault entries
/// - `vault unlock` - Unlock the shared password vault
/// - `vault lock` - Lock the shared password vault
/// - `vault status` - Show shared password vault status
/// - `vault set-master-password` - Create or rotate the master password
/// - `--pass-entry <name>` - Override the password entry for a direct launch
/// - `ssh_args` - All remaining arguments are passed to SSH
///
/// # Examples
/// ```text
/// cossh                              # Launch interactive session manager (default when no args)
/// cossh -d                           # Launch interactive session manager with safe debug enabled
/// cossh -dd user@example.com         # Raw debug enabled (may log terminal content and secrets)
/// cossh vault init                   # Initialize the password vault
/// cossh vault add office_fw     # Create/update password vault entry
/// cossh vault list                   # List password vault entries
/// cossh vault unlock                 # Unlock the shared password vault
/// cossh -d user@example.com          # Safe debug enabled
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

    if parsed.vault_command.is_none() && !parsed.interactive && parsed.ssh_args.is_empty() {
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
