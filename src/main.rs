/*
TODO:
    - Change debug logging call to use log level
    - Clean comments
    - Add more error handling
    - Go through each file and clean up use and crate imports to all have the same format
    - Improve error support to expand error handling across all modules for clean logging?
*/
use csh::{Result, args, config, log, log_debug, process};

use std::process::ExitCode;

fn main() -> Result<ExitCode> {
    // test::prompt_tests();

    let args = args::main_args();

    if config::SESSION_CONFIG.read().unwrap().settings.show_title {
        let title = [
            " ",
            "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
            "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
            "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
            "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
            "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
            "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
            "\x1b[31mVersion: \x1b[33m1.0\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m",
            " ",
        ];

        for (_, line) in title.iter().enumerate() {
            println!("{}\x1b[0m", line);
        }
    }

    // Initialize logging
    let logger = log::Logger::new();
    if args.debug || config::SESSION_CONFIG.read().unwrap().settings.debug_mode {
        logger.enable_debug();
        if let Err(err) = logger.log_debug("Debug mode enabled") {
            eprintln!("❌ Failed to initialize debug logging: {}", err);
            return Ok(ExitCode::FAILURE);
        }
    }

    if args.ssh_logging || config::SESSION_CONFIG.read().unwrap().settings.ssh_logging {
        logger.enable_ssh_logging();
        if let Err(err) = logger.log_debug("SSH logging enabled") {
            eprintln!("❌ Failed to initialize SSH logging: {}", err);
            return Ok(ExitCode::FAILURE);
        }
        let session_hostname = args
            .ssh_args
            .get(args.ssh_args.len() - 1)
            .map(|arg| arg.splitn(2, '@').nth(1).unwrap_or(arg))
            .unwrap_or("unknown");
        config::SESSION_CONFIG.write().unwrap().metadata.session_name = session_hostname.to_string();
    }

    drop(logger); // Release the lock on the logger

    // Starts the config file watcher in the background under the _watcher context
    let _watcher = config::config_watcher();

    // Start the process with the provided arguments and begin processing output
    process::process_handler(args.ssh_args)
}