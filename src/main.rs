use cossh::{Result, args, config, log, log_debug, log_error, log_info, process};
use std::process::ExitCode;

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
        return Some(arg.split_once('@').map_or_else(|| arg.to_string(), |(_, host)| host.to_string()));
    }

    None
}

fn main() -> Result<ExitCode> {
    let args = args::main_args();

    let logger = log::Logger::new();

    // Enable debug logging initially to capture config load
    logger.enable_debug();
    log_info!("color-ssh v0.5.9 starting");

    if let Err(err) = config::init_session_config(args.profile.clone()) {
        eprintln!("Failed to initialize config: {}", err);
        std::process::exit(1);
    }

    // Get global settings from config
    let (debug_from_config, ssh_log_from_config, show_title) = {
        let config_guard = config::get_config().read().unwrap();
        (
            config_guard.settings.debug_mode,
            config_guard.settings.ssh_logging,
            config_guard.settings.show_title,
        )
    };

    // Determine final debug mode
    let final_debug = args.debug || debug_from_config;
    let final_ssh_log = args.ssh_logging || ssh_log_from_config;

    if final_debug {
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
            "\x1b[31mVersion: \x1b[33m0.5.9\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m",
            " ",
        ];

        for line in title.iter() {
            println!("{}\x1b[0m", line);
        }
    }

    // Configure SSH session logging
    if logger.is_ssh_logging_enabled() {
        let session_hostname = extract_ssh_destination(&args.ssh_args).unwrap_or_else(|| "unknown".to_string());

        config::get_config().write().unwrap().metadata.session_name = session_hostname.to_string();
        log_debug!("Session name set to: {}", session_hostname);
    }

    // Start the config file watcher in the background
    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    // Start the SSH process
    log_info!("Launching SSH process handler");
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {}", err);
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    Ok(exit_code)
}
