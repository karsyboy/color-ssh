/*
TODO:
    - Change debug logging call to use log level
    - Clean comments
    - Add more error handling
    - Go through each file and clean up use and crate imports to all have the same format
    - Improve error support to expand error handling across all modules for clean logging?
*/
use csh::{Result, args, config, log, process, vault};

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
            eprintln!("Failed to initialize debug logging: {}", err);
            return Ok(ExitCode::FAILURE);
        }
    }

    if (args.ssh_logging || config::SESSION_CONFIG.read().unwrap().settings.ssh_logging) && args.vault_command.is_none() {
        logger.enable_ssh_logging();
        if let Err(err) = logger.log_debug("SSH logging enabled") {
            eprintln!("Failed to initialize SSH logging: {}", err);
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

    // Handle vault commands if they are present
    if args.vault_command.is_some() {
        if let Err(err) = vault::vault_handler(args.vault_command.clone().unwrap()) {
            eprintln!("Vault handler error: {}", err);
            return Ok(ExitCode::FAILURE);
        }
        return Ok(ExitCode::SUCCESS);
    }

    // Starts the config file watcher in the background under the _watcher context
    let _watcher = config::config_watcher();

    // Start the process with the provided arguments and begin processing output
    process::process_handler(args.ssh_args)
}

// mod test {
//     use csh::ui::Prompt;

//     pub fn prompt_tests() {
//         let mut prompt = Prompt::default();

//         prompt.set_help_msg(false);

//         let yes_no = prompt.yes_no_prompt("Do you want to continue", true);
//         println!("Yes/No: {}", yes_no);
//         println!("");

//         let true_false = prompt.true_false_prompt("Do you want to continue", false);
//         println!("True/False: {}", true_false);
//         println!("");

//         let name = prompt.validated_input_prompt("Enter a name", ".*", "Name not entered");
//         println!("Name: {}", name);
//         println!("");

//         let password = prompt.password_prompt().unwrap();
//         println!("Password: {}", password);
//         println!("");

//         let options = vec!["Option 1", "Option 2", "Option 3"];
//         let selected = prompt.selectable_prompt("Select an Option: ", &options, true);
//         println!("Selected: {}", selected.unwrap());
//         println!("");

//         let selected = prompt.selectable_prompt("Select an Option: ", &options, false);
//         println!("Selected: {}", selected.unwrap());
//         println!("");

//         std::process::exit(0);
//     }
// }
