use cossh::auth::pass::{self, PassCache, PassResolveResult};
use cossh::{Result, args, config, log, log_debug, log_error, log_info, process, ssh_config, tui};
use std::process::ExitCode;

const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));
const SKIP_PASS_RESOLVE_ENV: &str = "COSSH_SKIP_PASS_RESOLVE";

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

fn pass_key_for_destination_from_hosts(destination: &str, hosts: &[ssh_config::SshHost]) -> Option<String> {
    hosts.iter().find(|host| host.name == destination).and_then(|host| host.pass_key.clone())
}

fn pass_key_for_destination(destination: &str) -> Option<String> {
    let config_path = ssh_config::get_default_ssh_config_path()?;
    if !config_path.exists() {
        return None;
    }
    let hosts = match ssh_config::parse_ssh_config(&config_path) {
        Ok(hosts) => hosts,
        Err(err) => {
            log_debug!("Failed to parse SSH config for #_pass lookup: {}", err);
            return None;
        }
    };
    pass_key_for_destination_from_hosts(destination, &hosts)
}

fn is_add_pass_mode(args: &args::MainArgs) -> bool {
    args.add_pass.is_some()
}

fn skip_pass_resolution_from_env(value: Option<&str>) -> bool {
    value.is_some_and(|raw| matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn skip_pass_resolution_for_context(skip_env: Option<&str>, session_name_env: Option<&str>) -> bool {
    skip_pass_resolution_from_env(skip_env) && session_name_env.is_some_and(|value| !value.trim().is_empty())
}

fn should_skip_pass_resolution() -> bool {
    skip_pass_resolution_for_context(
        std::env::var(SKIP_PASS_RESOLVE_ENV).ok().as_deref(),
        std::env::var("COSSH_SESSION_NAME").ok().as_deref(),
    )
}

fn run_add_pass_cli(pass_name: &str) -> ExitCode {
    match pass::create_pass_key_interactive(pass_name) {
        Ok(path) => {
            println!("Saved encrypted pass key: {}", path.display());
            println!("Use in ~/.ssh/config: #_pass {}", pass_name);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to add pass key: {}", err);
            ExitCode::from(1)
        }
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

    if is_add_pass_mode(&args)
        && let Some(pass_name) = args.add_pass.as_deref()
    {
        return Ok(run_add_pass_cli(pass_name));
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

    let mut pass_password: Option<String> = None;
    if should_skip_pass_resolution() {
        log_debug!("Skipping #_pass resolution due to {}", SKIP_PASS_RESOLVE_ENV);
    } else if !args.is_non_interactive
        && let Some(destination) = extract_ssh_destination(&args.ssh_args)
        && let Some(pass_key) = pass_key_for_destination(&destination)
    {
        let mut pass_cache = PassCache::default();
        match pass::resolve_pass_key(&pass_key, &mut pass_cache) {
            PassResolveResult::Ready(password) => {
                pass_password = Some(password);
                log_debug!("Pass auto-login enabled for destination {}", destination);
            }
            PassResolveResult::Disabled => {}
            PassResolveResult::Fallback(reason) => {
                log_debug!("Pass auto-login fallback for destination {}: {:?}", destination, reason);
                eprintln!("{}", pass::fallback_notice(reason));
            }
        }
    }

    // Start the SSH process
    log_info!("Launching SSH process handler");
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive, pass_password).map_err(|err| {
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
