use cossh::{Result, args, auth, config, log, log_debug, log_debug_raw, log_error, log_info, log_warn, process, tui};
use std::process::ExitCode;
use std::sync::Once;

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

fn resolve_logging_settings(args: &args::MainArgs, debug_from_config: bool, ssh_log_from_config: bool) -> (log::DebugVerbosity, bool) {
    let cli_debug = log::DebugVerbosity::from_count(args.debug_count);
    let config_debug = if debug_from_config {
        log::DebugVerbosity::Safe
    } else {
        log::DebugVerbosity::Off
    };
    if args.test_mode {
        (cli_debug, args.ssh_logging)
    } else {
        (cli_debug.max(config_debug), args.ssh_logging || ssh_log_from_config)
    }
}

fn emit_raw_debug_warning_once() {
    static RAW_DEBUG_WARNING: Once = Once::new();
    const RAW_DEBUG_WARNING_MESSAGE: &str =
        "Raw debug logging is enabled and may capture terminal content, CLI arguments, and secrets in ~/.color-ssh/logs/cossh.log.";

    RAW_DEBUG_WARNING.call_once(|| {
        eprintln!("[color-ssh] {}", RAW_DEBUG_WARNING_MESSAGE);
        log_warn!("{}", RAW_DEBUG_WARNING_MESSAGE);
    });
}

fn initialize_config_for_vault_command(profile: Option<String>) -> Result<()> {
    config::init_session_config(profile)?;
    Ok(())
}

fn main() -> Result<ExitCode> {
    if auth::transport::is_internal_askpass_invocation() {
        return Ok(auth::run_internal_askpass());
    }

    let args = args::main_args();

    let logger = log::Logger::new();

    // Enable debug logging only when explicitly requested on CLI.
    // Config-based debug mode is applied after loading config.
    if args.debug_count > 0 {
        logger.enable_debug_with_verbosity(log::DebugVerbosity::from_count(args.debug_count));
    }
    log_info!("color-ssh {} starting", APP_VERSION);

    if args.agent_serve {
        auth::agent::run_server().map_err(|err| {
            log_error!("Password vault agent failed: {}", err);
            std::io::Error::other(err.to_string())
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(vault_command) = args.vault_command.as_ref() {
        if let Err(err) = initialize_config_for_vault_command(args.profile.clone()) {
            log_error!("Failed to initialize config for vault command: {}", err);
            eprintln!("Failed to initialize config: {err}");
            let _ = logger.flush_debug();
            std::process::exit(1);
        }
        return Ok(auth::run_vault_command(vault_command));
    }

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
        if final_debug >= log::DebugVerbosity::Safe {
            if logger.debug_verbosity() != final_debug {
                logger.enable_debug_with_verbosity(final_debug);
            }
            if final_debug >= log::DebugVerbosity::Raw {
                emit_raw_debug_warning_once();
            }
            if args.debug_count >= 2 {
                log_debug!("Raw debug mode enabled via CLI arguments");
            } else if args.debug_count == 1 {
                log_debug!("Safe debug mode enabled via CLI argument");
            } else {
                log_debug!("Safe debug mode enabled via config file");
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

    if final_debug >= log::DebugVerbosity::Safe {
        if logger.debug_verbosity() != final_debug {
            logger.enable_debug_with_verbosity(final_debug);
        }
        if final_debug >= log::DebugVerbosity::Raw {
            emit_raw_debug_warning_once();
        }
        if args.debug_count >= 2 {
            log_debug!("Raw debug mode enabled via CLI arguments");
        } else if args.debug_count == 1 {
            log_debug!("Safe debug mode enabled via CLI argument");
        } else {
            log_debug!("Safe debug mode enabled via config file");
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

    log_debug!(
        "Parsed arguments summary: interactive={} ssh_arg_count={} pass_entry_override={} vault_command={} profile_set={} agent_serve={} test_mode={}",
        args.interactive,
        args.ssh_args.len(),
        args.pass_entry.is_some(),
        args.vault_command.is_some(),
        args.profile.is_some(),
        args.agent_serve,
        args.test_mode
    );
    log_debug_raw!("Parsed arguments: {:?}", args);

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
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive, args.pass_entry).map_err(|err| {
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
#[path = "test/main.rs"]
mod tests;
