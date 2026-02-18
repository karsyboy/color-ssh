//! Command-line argument parsing
//!
//! Parses CLI arguments using the clap library and provides structured access
//! to user-provided options.

use clap::{Arg, Command};
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
}

fn build_cli_command() -> Command {
    Command::new("cossh")
        .version("v0.6.0")
        .author("@karsyboy")
        .about("A Rust-based SSH client wrapper with syntax highlighting and logging capabilities")
        .propagate_version(true)
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
        .arg(Arg::new("ssh_args").help("SSH arguments to forward to the SSH command").num_args(1..))
        .after_help(
            r"
cossh                                              # Launch interactive session manager
cossh -d                                           # Launch interactive session manager with debug enabled
cossh -d user@example.com                          # Debug mode enabled
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

fn parse_main_args_from<I, T>(cmd: &Command, raw_args: I) -> MainArgs
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let raw_args: Vec<OsString> = raw_args.into_iter().map(Into::into).collect();
    let matches = cmd.clone().get_matches_from(raw_args.clone());

    // Retrieve SSH arguments to forward.
    let ssh_args: Vec<String> = matches.get_many::<String>("ssh_args").map(|vals| vals.cloned().collect()).unwrap_or_default();
    let debug = matches.get_flag("debug");
    let ssh_logging = matches.get_flag("log");
    let test_mode = matches.get_flag("test");
    let profile = matches.get_one::<String>("profile").cloned().filter(|profile_name| !profile_name.is_empty());
    let no_user_args = raw_args.len() <= 1;
    let debug_only = debug && !ssh_logging && profile.is_none() && ssh_args.is_empty();
    let interactive = no_user_args || debug_only;

    MainArgs {
        debug,
        ssh_logging,
        test_mode,
        interactive,
        profile,
        is_non_interactive: detect_non_interactive_ssh_args(&ssh_args),
        ssh_args,
    }
}

/// Parses command-line arguments using clap.
///
/// # Arguments Supported
/// - `-d, --debug` - Enable debug mode with detailed logging
/// - `-l, --log` - Enable SSH session logging
/// - `-t, --test` - Ignore config logging settings and use only CLI `-d/-l` logging flags
/// - `ssh_args` - All remaining arguments are passed to SSH
///
/// # Examples
/// ```text
/// cossh                              # Launch interactive session manager (default when no args)
/// cossh -d                           # Launch interactive session manager with debug enabled
/// cossh -d user@example.com          # Debug mode enabled
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

    if !parsed.interactive && parsed.ssh_args.is_empty() {
        let mut help_cmd = cmd;
        let _ = help_cmd.print_long_help();
        println!();
        std::process::exit(2);
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::{build_cli_command, detect_non_interactive_ssh_args, parse_main_args_from};

    #[test]
    fn enters_interactive_mode_with_no_user_args() {
        let cmd = build_cli_command();
        let parsed = parse_main_args_from(&cmd, ["cossh"]);
        assert!(parsed.interactive);
        assert!(parsed.ssh_args.is_empty());
    }

    #[test]
    fn enters_interactive_mode_for_debug_only() {
        let cmd = build_cli_command();
        let parsed = parse_main_args_from(&cmd, ["cossh", "-d"]);
        assert!(parsed.interactive);
        assert!(parsed.debug);
        assert!(parsed.ssh_args.is_empty());
    }

    #[test]
    fn does_not_enter_interactive_mode_when_connect_target_is_present() {
        let cmd = build_cli_command();
        let parsed = parse_main_args_from(&cmd, ["cossh", "-d", "user@example.com"]);
        assert!(!parsed.interactive);
        assert_eq!(parsed.ssh_args, vec!["user@example.com".to_string()]);
    }

    #[test]
    fn detects_non_interactive_passthrough_flags() {
        for flag in ["-G", "-V", "-Q", "-O"] {
            let ssh_args = vec![flag.to_string(), "example.com".to_string()];
            assert!(detect_non_interactive_ssh_args(&ssh_args), "flag {flag} should be passthrough");
        }
    }

    #[test]
    fn does_not_detect_connection_mode_flags_as_passthrough() {
        for flag in ["-T", "-N", "-n", "-f", "-W"] {
            let ssh_args = vec![flag.to_string(), "example.com".to_string()];
            assert!(
                !detect_non_interactive_ssh_args(&ssh_args),
                "flag {flag} should stay in normal connection pipeline"
            );
        }
        let ssh_args = vec!["user@example.com".to_string()];
        assert!(!detect_non_interactive_ssh_args(&ssh_args));
    }

    #[test]
    fn parses_test_mode_and_combined_short_flags() {
        let cmd = build_cli_command();
        let parsed = parse_main_args_from(&cmd, ["cossh", "-tld", "localhost"]);

        assert!(parsed.test_mode);
        assert!(parsed.debug);
        assert!(parsed.ssh_logging);
        assert!(!parsed.interactive);
        assert_eq!(parsed.ssh_args, vec!["localhost".to_string()]);
    }
}
