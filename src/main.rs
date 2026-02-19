use cossh::{Result, args, config, log, log_debug, log_error, log_info, process, tui};
use std::process::ExitCode;

const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Extracts the SSH destination hostname from the provided SSH arguments returns hostname or none
fn extract_ssh_destination(ssh_args: &[String]) -> Option<String> {
    // SSH flags that take an argument based off ssh version "OpenSSH_10.2p1"
    let flags_with_args = [
        "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
    ];

    let mut skip_next = false;

    for arg in ssh_args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg.starts_with('-') {
            if flags_with_args.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }

        // Extract just the hostname part after @ if it exist
        return Some(arg.split_once('@').map_or_else(|| arg.clone(), |(_, host)| host.to_string()));
    }

    None
}

fn resolve_logging_settings(args: &args::MainArgs, debug_from_config: bool, ssh_log_from_config: bool) -> (bool, bool) {
    if args.test_mode {
        (args.debug, args.ssh_logging)
    } else {
        (args.debug || debug_from_config, args.ssh_logging || ssh_log_from_config)
    }
}

fn main() -> Result<ExitCode> {
    let args = args::main_args();

    let logger = log::Logger::new();

    // Enable debug logging only when explicitly requested on CLI.
    // Config-based debug mode is applied after loading config.
    if args.debug {
        logger.enable_debug();
    }
    log_info!("color-ssh {} starting", APP_VERSION);

    // If interactive mode is requested, launch the session manager
    if args.interactive {
        log_info!("Launching interactive session manager");
        // Init config so session manager can read interactive settings (if configured)
        let debug_from_config = if config::init_session_config(args.profile.clone()).is_ok() {
            match config::get_config().read() {
                Ok(config_guard) => config_guard.settings.debug_mode,
                Err(poisoned) => {
                    log_error!("Configuration lock poisoned while reading interactive debug setting; continuing with recovered state");
                    poisoned.into_inner().settings.debug_mode
                }
            }
        } else {
            false
        };
        let (final_debug, _) = resolve_logging_settings(&args, debug_from_config, false);
        if final_debug {
            if !logger.is_debug_enabled() {
                logger.enable_debug();
            }
            if args.debug {
                log_debug!("Debug mode enabled via CLI argument");
            } else {
                log_debug!("Debug mode enabled via config file");
            }
        } else {
            logger.disable_debug();
        }

        if let Err(err) = tui::run_session_manager() {
            eprintln!("Session manager error: {err}");
            let _ = logger.flush_debug();
            std::process::exit(1);
        }
        let _ = logger.flush_debug();
        return Ok(ExitCode::SUCCESS);
    }

    if let Err(err) = config::init_session_config(args.profile.clone()) {
        eprintln!("Failed to initialize config: {err}");
        let _ = logger.flush_debug();
        std::process::exit(1);
    }

    // Get global settings from config
    let (debug_from_config, ssh_log_from_config, show_title) = {
        match config::get_config().read() {
            Ok(config_guard) => (
                config_guard.settings.debug_mode,
                config_guard.settings.ssh_logging,
                config_guard.settings.show_title,
            ),
            Err(poisoned) => {
                log_error!("Configuration lock poisoned while reading global settings; continuing with recovered state");
                let config_guard = poisoned.into_inner();
                (
                    config_guard.settings.debug_mode,
                    config_guard.settings.ssh_logging,
                    config_guard.settings.show_title,
                )
            }
        }
    };

    // Determine final logging mode
    let (final_debug, final_ssh_log) = resolve_logging_settings(&args, debug_from_config, ssh_log_from_config);

    if final_debug {
        if !logger.is_debug_enabled() {
            logger.enable_debug();
        }
        if args.debug {
            log_debug!("Debug mode enabled via CLI argument");
        } else {
            log_debug!("Debug mode enabled via config file");
        }
    } else {
        log_debug!("Debug mode not requested, disabling after initial config load");
        logger.disable_debug();
    }

    // Enable SSH logging
    if final_ssh_log {
        logger.enable_ssh_logging();
        if args.ssh_logging {
            log_info!("SSH logging enabled via CLI argument");
        } else {
            log_info!("SSH logging enabled via config file");
        }
    }

    log_debug!("Parsed arguments: {:?}", args);

    if show_title {
        log_debug!("Banner display enabled in config, printing banner");
        let title = [
            " ",
            "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
            "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
            "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
            "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
            "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
            "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
            concat!(
                "\x1b[31mVersion: \x1b[33mv",
                env!("CARGO_PKG_VERSION"),
                "\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m"
            ),
            " ",
        ];

        for line in &title {
            println!("{line}\x1b[0m");
        }
    }

    // Configure SSH session logging
    if logger.is_ssh_logging_enabled() {
        let session_hostname = extract_ssh_destination(&args.ssh_args).unwrap_or_else(|| "unknown".to_string());

        // Use COSSH_SESSION_NAME env var if set (from session manager tabs), otherwise use hostname
        let session_name = std::env::var("COSSH_SESSION_NAME").unwrap_or_else(|_| session_hostname.clone());
        let session_name = log::sanitize_session_name(&session_name);
        match config::get_config().write() {
            Ok(mut config_guard) => {
                config_guard.metadata.session_name = session_name.clone();
            }
            Err(poisoned) => {
                log_error!("Configuration lock poisoned while setting session name; continuing with recovered state");
                let mut config_guard = poisoned.into_inner();
                config_guard.metadata.session_name = session_name.clone();
            }
        }
        log_debug!("Session name set to: {session_name}");
    }

    // Start the config file watcher in the background
    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    // Start the SSH process
    log_info!("Launching SSH process handler");
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        let _ = logger.flush_debug();
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    let _ = logger.flush_debug();
    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::{extract_ssh_destination, resolve_logging_settings};
    use cossh::args::MainArgs;

    fn base_args(debug: bool, ssh_logging: bool, test_mode: bool) -> MainArgs {
        MainArgs {
            debug,
            ssh_logging,
            test_mode,
            ssh_args: vec!["localhost".to_string()],
            profile: None,
            is_non_interactive: false,
            interactive: false,
        }
    }

    #[test]
    fn extracts_plain_destination_host() {
        let args = vec!["example.com".to_string()];
        assert_eq!(extract_ssh_destination(&args).as_deref(), Some("example.com"));
    }

    #[test]
    fn extracts_host_from_user_at_host_syntax() {
        let args = vec!["alice@example.com".to_string()];
        assert_eq!(extract_ssh_destination(&args).as_deref(), Some("example.com"));
    }

    #[test]
    fn skips_flags_that_consume_values_before_destination() {
        let args = vec![
            "-p".to_string(),
            "2222".to_string(),
            "-J".to_string(),
            "jump.example.com".to_string(),
            "target.example.com".to_string(),
        ];
        assert_eq!(extract_ssh_destination(&args).as_deref(), Some("target.example.com"));
    }

    #[test]
    fn returns_none_when_only_value_consuming_flags_are_present() {
        let args = vec!["-W".to_string(), "localhost:22".to_string()];
        assert_eq!(extract_ssh_destination(&args), None);
    }

    #[test]
    fn test_mode_uses_only_cli_logging_flags() {
        let args = base_args(false, false, true);
        assert_eq!(resolve_logging_settings(&args, true, true), (false, false));

        let args = base_args(true, false, true);
        assert_eq!(resolve_logging_settings(&args, false, true), (true, false));

        let args = base_args(false, true, true);
        assert_eq!(resolve_logging_settings(&args, true, false), (false, true));
    }

    #[test]
    fn normal_mode_merges_cli_and_config_logging_flags() {
        let args = base_args(false, false, false);
        assert_eq!(resolve_logging_settings(&args, true, true), (true, true));

        let args = base_args(true, false, false);
        assert_eq!(resolve_logging_settings(&args, false, false), (true, false));

        let args = base_args(false, true, false);
        assert_eq!(resolve_logging_settings(&args, false, false), (false, true));
    }
}
