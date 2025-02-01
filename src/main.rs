use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

// Imports CSH specific modules
mod cli;
mod config;
mod highlighter;
mod logging;
mod ssh;
mod vault;

use cli::{parse_args, SSH_LOGGING};
use config::{compile_rules, load_config, Config};
use highlighter::process_chunk;
use logging::{enable_debug_mode, log_debug, log_ssh_output, DEBUG_MODE};
use ssh::spawn_ssh;

fn main() -> io::Result<()> {
    // Get the command-line arguments from the clap function in cli.rs
    let args = parse_args();

    // If debugging, log the provided SSH arguments for verification
    if DEBUG_MODE.load(Ordering::Relaxed) {
        log_debug(&format!(
            "Debug mode: {}",
            DEBUG_MODE.load(Ordering::Relaxed)
        ))
        .unwrap();
        log_debug(&format!(
            "SSH logging: {}",
            SSH_LOGGING.load(Ordering::Relaxed)
        ))
        .unwrap();
        log_debug(&format!("SSH arguments: {:?}", args)).unwrap();
    }

    // Load CSH configuration from the YAML file
    let config: Config = load_config().expect("Failed to load configuration.");

    // Compile regex rules and associate them with the corresponding colors
    let rules = compile_rules(&config);

    // If debugging, log the compiled rules
    if DEBUG_MODE.load(Ordering::Relaxed) {
        log_debug("Compiled rules:").unwrap();
        for (i, (regex, color)) in rules.iter().enumerate() {
            log_debug(&format!(
                "  Rule {}: regex = {:?}, color = {:?}",
                i + 1,
                regex,
                color
            ))
            .unwrap();
        }
    }

    // Launch the SSH process with the provided arguments
    let mut child = spawn_ssh(&args)?;

    // Capture the SSH output
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let reset_color = "\x1b[0m"; // ANSI reset color sequence

    // Spawn a thread to process the output chunks
    let mut chunk_id = 0;
    thread::spawn(move || {
        while let Ok(chunk) = rx.recv() {
            // Process the chunk and apply highlighting
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
        if SSH_LOGGING.load(Ordering::Relaxed) {
            log_ssh_output(&chunk, &args).unwrap();
        }
        tx.send(chunk)
            .expect("Failed to send data to processing thread");
    }

    // Wait for the SSH process to finish and exit with the process's status code
    let status = child.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}
