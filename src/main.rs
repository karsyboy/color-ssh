use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[cfg(unix)]
//use socket::unix_socket;

#[cfg(windows)]
use socket::windows_pipe;

// Imports CSH specific modules
mod cli;
mod config;
mod highlighter;
mod logging;
// mod socket;
mod watcher;
mod ssh;
mod vault;

use cli::{parse_args, SSH_LOGGING};
use config::{config_watcher, COMPILED_RULES};
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

    let _watcher = config_watcher();

    // Launch the SSH process with the provided arguments
    let mut child = spawn_ssh(&args)?;
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    // Create needed values for chunk reading thread
    let proc_reset_color = "\x1b[0m"; // ANSI reset color sequence
    let proc_compiled_rules = Arc::clone(&COMPILED_RULES);

    // Spawn thread for reading chunks sent by ssh
    // This is the main thread that reads the chunks and applies the highlighter
    thread::spawn(move || {
        let mut chunk_id = 0;
        while let Ok(chunk) = rx.recv() {
            let rules = proc_compiled_rules.read().unwrap().clone();
            let processed = process_chunk(chunk, chunk_id, &rules, proc_reset_color);
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
