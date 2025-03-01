/*
TODO:
    - Change debug logging call to use log level
    - Clean comments
    - Add more error handling
    - Go through each file and clean up use and crate imports to all have the same format
    - Split main.rs into app.rs and main.rs. Main.rs will act as an entry point and app.rs will contain the processing functions.
    - Improve error support to expand error handling across all modules for clean logging?
*/

use std::{
    io::{self, BufReader, Read, Write},
    process::ExitCode,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use csh::{
    cli::main_args,
    config::{watcher::config_watcher, CONFIG},
    highlighter::process_chunk,
    log_debug, log_ssh,
    logging::Logger,
    process::spawn_ssh,
    vault::vault_handler,
    Result,
};

fn main() -> Result<ExitCode> {
    // Get the command-line arguments from the clap function in cli.rs
    let args = main_args();

    let title = [
        " ",
        "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
        "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
        "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
        "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
        "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
        "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
        "\x1b[31mVersion: \x1b[33m1.0\x1b[0m    \x1b[31mBy: \x1b[32m@Karyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m",
        " ",
    ];

    for (_, line) in title.iter().enumerate() {
        println!("{}\x1b[0m", line); // Reset color after each line
    }

    // Initialize logging in a separate scope so the lock is released
    let logger = Logger::new();
    if args.debug || CONFIG.read().unwrap().settings.debug_mode {
        logger.enable_debug();
        if let Err(e) = logger.log_debug("Debug mode enabled") {
            eprintln!("Failed to initialize debug logging: {}", e);
            return Ok(ExitCode::FAILURE);
        }
    }

    if args.vault_command.is_some() {
        let _ = vault_handler(args.vault_command.clone().unwrap());
        return Ok(ExitCode::SUCCESS);
    }

    if args.ssh_logging || CONFIG.read().unwrap().settings.ssh_logging {
        logger.enable_ssh_logging();
        if let Err(e) = logger.log_debug("SSH logging enabled") {
            eprintln!("Failed to initialize SSH logging: {}", e);
            return Ok(ExitCode::FAILURE);
        }

        // Set the session name for the ssh log file based on the first argument
        // Note: this may need to change if the user provides a different session name
        let session_hostname = args
            .ssh_args
            .get(args.ssh_args.len() - 1)
            .map(|arg| arg.splitn(2, '@').nth(1).unwrap_or(arg))
            .unwrap_or("unknown");
        CONFIG.write().unwrap().metadata.session_name = session_hostname.to_string();
    }

    drop(logger); // Release the lock on the logger

    // Load the configuration file
    log_debug!("SSH arguments: {:?}", args.ssh_args);

    // Starts the config file watcher in the background under the _watcher context
    let _watcher = config_watcher();

    // Launch the SSH process with the provided arguments
    let mut child = spawn_ssh(&args.ssh_args).expect("Failed to spawn SSH process\r");
    let stdout = child.stdout.take().expect("Failed to capture stdout\r");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    // Create needed values for chunk reading thread
    let reset_color = "\x1b[0m"; // ANSI reset color sequence

    // Spawn thread for reading chunks sent by ssh
    // This is the main thread that reads the chunks and applies the highlighter
    thread::spawn(move || {
        let mut chunk_id = 0;
        while let Ok(chunk) = rx.recv() {
            let rules = CONFIG.read().unwrap().metadata.compiled_rules.clone();
            let processed = process_chunk(chunk, chunk_id, &rules, reset_color);
            chunk_id += 1;
            print!("{}", processed); // Print the processed chunk
            io::stdout().flush().unwrap(); // Flush to ensure immediate display
        }
    });

    // Buffer for reading data from SSH output
    let mut buffer = [0; 4096];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break; // Exit loop when EOF is reached
        }
        // Convert the read data to a String and send it to the processing thread
        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
        log_ssh!("{}", chunk);
        tx.send(chunk)
            .expect("Failed to send data to processing thread\r");
    }

    // Wait for the SSH process to finish and use its status code
    let status = child.wait()?;
    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
    }
}
