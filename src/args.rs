//! Command-line argument parsing
//!
//! Parses CLI arguments using the clap library and provides structured access
//! to user-provided options.

use crate::{ssh_args, validation};
use clap::{Arg, Command, error::ErrorKind};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdpCommandArgs {
    /// Target host or configured alias.
    pub target: String,
    /// Optional username override.
    pub user: Option<String>,
    /// Optional domain override.
    pub domain: Option<String>,
    /// Optional port override.
    pub port: Option<u16>,
    /// Additional arguments forwarded to `xfreerdp3` or `xfreerdp`.
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshCommandArgs {
    /// Arguments to pass through to the SSH command.
    pub ssh_args: Vec<String>,
    /// Whether the SSH command is non-interactive (e.g., -G, -V, -O, -Q).
    pub is_non_interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolCommand {
    Ssh(SshCommandArgs),
    Rdp(RdpCommandArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainCommand {
    Protocol(ProtocolCommand),
    Vault(VaultCommand),
    MigrateInventory,
    AgentServe,
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
    /// Argument to pass for configuration profiles
    pub profile: Option<String>,
    /// Launch interactive session manager TUI
    pub interactive: bool,
    /// Override the password entry to use for a direct protocol launch.
    pub pass_entry: Option<String>,
    /// Selected command, if any.
    pub command: Option<MainCommand>,
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
                .value_parser(clap::builder::ValueParser::new(validation::parse_profile_name))
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
                .help("Override the password vault entry used for a direct protocol launch")
                .num_args(1)
                .value_name("name")
                .value_parser(clap::builder::ValueParser::new(validation::parse_vault_entry_name)),
        )
        .arg(
            Arg::new("migrate")
                .long("migrate")
                .help("Migrate ~/.ssh/config host entries into ~/.color-ssh/cossh-inventory.yaml")
                .action(clap::ArgAction::SetTrue)
                .conflicts_with_all(["log", "profile", "test", "pass_entry"]),
        )
        .subcommand(
            Command::new("ssh")
                .about("Launch an SSH session by forwarding arguments to the SSH command")
                .arg(
                    Arg::new("ssh_args")
                        .help("SSH arguments to forward to the SSH command")
                        .required(true)
                        .num_args(1..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true),
                ),
        )
        .subcommand(
            Command::new("rdp")
                .about("Launch an RDP session using xfreerdp3 or xfreerdp")
                .arg(Arg::new("target").help("RDP target host or configured alias").required(true))
                .arg(Arg::new("user").short('u').long("user").help("Override the RDP username").num_args(1))
                .arg(Arg::new("domain").short('D').long("domain").help("Override the RDP domain").num_args(1))
                .arg(
                    Arg::new("port")
                        .short('p')
                        .long("port")
                        .help("Override the RDP port")
                        .num_args(1)
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(
                    Arg::new("rdp_args")
                        .help("Additional xfreerdp3/xfreerdp arguments")
                        .num_args(0..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true),
                ),
        )
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
                            .value_parser(clap::builder::ValueParser::new(validation::parse_vault_entry_name)),
                    ),
                )
                .subcommand(
                    Command::new("remove").about("Remove a password vault entry").arg(
                        Arg::new("name")
                            .help("Password entry name")
                            .required(true)
                            .value_parser(clap::builder::ValueParser::new(validation::parse_vault_entry_name)),
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
cossh                                                     # Launch interactive session manager
cossh -d ssh user@example.com                             # Safe debug enabled
cossh --pass-entry office_fw <ssh/rdp> host.example.com   # Override the password entry for this launch
cossh -l ssh user@example.com                             # SSH logging enabled
cossh -l -P network ssh user@firewall.example.com         # Use 'network' config profile
cossh -l ssh user@host -p 2222                            # Both modes with SSH args
cossh ssh user@host -G                                    # Non-interactive command
cossh rdp desktop01                                       # Launch a configured RDP host
cossh --migrate                                           # Import ~/.ssh/config into the YAML inventory
",
        )
}

fn parse_ssh_command(ssh_matches: &clap::ArgMatches) -> Option<SshCommandArgs> {
    let ssh_args: Vec<String> = ssh_matches
        .get_many::<String>("ssh_args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    if ssh_args.is_empty() {
        return None;
    }

    Some(SshCommandArgs {
        is_non_interactive: ssh_args::is_non_interactive_ssh_invocation(&ssh_args),
        ssh_args,
    })
}

fn parse_vault_command(vault_matches: &clap::ArgMatches) -> Option<VaultCommand> {
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

fn parse_rdp_command(rdp_matches: &clap::ArgMatches) -> Option<RdpCommandArgs> {
    let target = rdp_matches.get_one::<String>("target")?.trim().to_string();
    if target.is_empty() {
        return None;
    }

    Some(RdpCommandArgs {
        target,
        user: rdp_matches.get_one::<String>("user").cloned().filter(|value| !value.trim().is_empty()),
        domain: rdp_matches.get_one::<String>("domain").cloned().filter(|value| !value.trim().is_empty()),
        port: rdp_matches.get_one::<u16>("port").copied(),
        extra_args: rdp_matches
            .get_many::<String>("rdp_args")
            .map(|values| values.cloned().collect())
            .unwrap_or_default(),
    })
}

fn parse_main_command(matches: &clap::ArgMatches) -> Option<MainCommand> {
    if matches.get_flag("migrate") {
        return Some(MainCommand::MigrateInventory);
    }

    match matches.subcommand()? {
        ("ssh", ssh_matches) => parse_ssh_command(ssh_matches).map(ProtocolCommand::Ssh).map(MainCommand::Protocol),
        ("rdp", rdp_matches) => parse_rdp_command(rdp_matches).map(ProtocolCommand::Rdp).map(MainCommand::Protocol),
        ("vault", vault_matches) => parse_vault_command(vault_matches).map(MainCommand::Vault),
        ("agent", agent_matches) if agent_matches.get_flag("serve") => Some(MainCommand::AgentServe),
        _ => None,
    }
}

fn validate_main_args(cmd: &Command, matches: &clap::ArgMatches, parsed: &MainArgs) -> Result<(), clap::Error> {
    if matches.get_flag("migrate") && matches.subcommand_name().is_some() {
        return Err(cmd
            .clone()
            .error(ErrorKind::ArgumentConflict, "`--migrate` cannot be combined with subcommands"));
    }

    if matches!(parsed.command, Some(MainCommand::MigrateInventory)) && parsed.interactive {
        return Err(cmd
            .clone()
            .error(ErrorKind::ArgumentConflict, "`--migrate` cannot be combined with interactive mode"));
    }

    Ok(())
}

fn parse_main_args_from<I, T>(cmd: &Command, raw_args: I) -> MainArgs
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    try_parse_main_args_from(cmd, raw_args).unwrap_or_else(|err| err.exit())
}

fn try_parse_main_args_from<I, T>(cmd: &Command, raw_args: I) -> Result<MainArgs, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw_args: Vec<OsString> = raw_args.into_iter().map(Into::into).collect();

    let matches = cmd.clone().try_get_matches_from(raw_args.clone())?;

    let debug_count = matches.get_count("debug");
    let ssh_logging = matches.get_flag("log");
    let test_mode = matches.get_flag("test");
    let profile = matches.get_one::<String>("profile").cloned().filter(|profile_name| !profile_name.is_empty());
    let pass_entry = matches.get_one::<String>("pass_entry").cloned().filter(|value| !value.is_empty());
    let command = parse_main_command(&matches);
    let no_user_args = raw_args.len() <= 1;
    let debug_only = debug_count > 0 && !ssh_logging && profile.is_none() && pass_entry.is_none() && command.is_none();
    let interactive = (no_user_args || debug_only) && command.is_none();

    let parsed = MainArgs {
        debug_count,
        ssh_logging,
        test_mode,
        profile,
        interactive,
        pass_entry,
        command,
    };
    validate_main_args(cmd, &matches, &parsed)?;
    Ok(parsed)
}

/// Parses command-line arguments using clap.
pub fn main_args() -> MainArgs {
    let cmd = build_cli_command();
    let parsed = parse_main_args_from(&cmd, std::env::args_os());

    if matches!(parsed.command, Some(MainCommand::AgentServe)) {
        return parsed;
    }

    if parsed.command.is_none() && !parsed.interactive {
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
