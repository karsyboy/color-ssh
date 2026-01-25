//! color-ssh (csh) - A Rust-based SSH client wrapper with syntax highlighting
//!
//! Main entry point that coordinates:
//! - Command-line argument parsing
//! - Logging initialization
//! - Configuration loading and watching
//! - SSH process spawning and management

use csh::{Result, args, config, log, log_debug, log_info, log_error, process};
use std::process::ExitCode;

fn main() -> Result<ExitCode> {
    // Parse command-line arguments
    let args = args::main_args();

    // Initialize logging system
    let logger = log::Logger::new();
    
    // ALWAYS enable debug logging initially to capture config load
    // We'll check the config settings and potentially disable it after
    logger.enable_debug();
    log_info!("color-ssh v0.5 starting");
    
    // Force initialization of SESSION_CONFIG - this will now be logged
    let (debug_from_config, ssh_log_from_config, show_title) = {
        let config_guard = config::SESSION_CONFIG.read().unwrap();
        (
            config_guard.settings.debug_mode,
            config_guard.settings.ssh_logging,
            config_guard.settings.show_title,
        )
    };
    
    // Determine final debug mode: CLI arg takes precedence, then config setting
    let _final_debug = args.debug || debug_from_config;
    let final_ssh_log = args.ssh_logging || ssh_log_from_config;
    
    // Log how debug mode was enabled (or if it should be disabled)
    if args.debug {
        log_debug!("Debug mode enabled via CLI argument");
    } else if debug_from_config {
        log_debug!("Debug mode enabled via config file");
    } else {
        // Neither CLI nor config wants debug - disable it after initial config load
        log_debug!("Debug mode not requested, disabling after initial config load");
        logger.disable_debug();
    }
    
    // Enable SSH logging if requested
    if final_ssh_log {
        logger.enable_ssh_logging();
        if args.ssh_logging {
            log_info!("SSH logging enabled via CLI argument");
        } else {
            log_info!("SSH logging enabled via config file");
        }
    }
    
    // Log parsed arguments (only if debug is still enabled)
    log_debug!("Parsed arguments: {:?}", args);

    // Display banner if enabled in config
    if show_title {
        let title = [
            " ",
            "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
            "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
            "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
            "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
            "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
            "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
            "\x1b[31mVersion: \x1b[33m0.5\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m",
            " ",
        ];

        for line in title.iter() {
            println!("{}\x1b[0m", line);
        }
    }

    // Configure SSH session logging if enabled
    if logger.is_ssh_logging_enabled() {
        
        // Extract hostname from SSH arguments for log file naming
        // Use the last argument which is typically the hostname/user@hostname
        let session_hostname = args
            .ssh_args
            .last()
            .map(|arg| arg.splitn(2, '@').nth(1).unwrap_or(arg))
            .unwrap_or("unknown");
        
        config::SESSION_CONFIG.write().unwrap().metadata.session_name = session_hostname.to_string();
        log_debug!("Session name set to: {}", session_hostname);
    }

    // Release the logger (drop the lock)
    drop(logger);

    // Start the config file watcher in the background
    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher();

    // Start the SSH process with the provided arguments and begin processing output
    log_info!("Launching SSH process handler");
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive)
        .map_err(|e| {
            log_error!("Process handler failed: {}", e);
            eprintln!("Process failed: {}", e);
            e
        })?;
    
    log_info!("color-ssh exiting with code: {:?}", exit_code);
    Ok(exit_code)
}