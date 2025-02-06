use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

#[cfg(unix)]
use socket::unix_socket;

#[cfg(windows)]
use socket::windows_pipe;

// Imports CSH specific modules
mod cli;
mod config;
mod highlighter;
mod logging;
mod socket;
mod ssh;
mod vault;

use cli::{parse_args, SSH_LOGGING};
use config::{compile_rules, load_config, COMPILED_RULES, CONFIG};
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

    // If debugging, log the compiled rules
    if DEBUG_MODE.load(Ordering::Relaxed) {
        log_debug("Compiled rules:").unwrap();
        for (i, (regex, color)) in COMPILED_RULES.read().unwrap().iter().enumerate() {
            log_debug(&format!(
                "  Rule {}: regex = {:?}, color = {:?}",
                i + 1,
                regex,
                color
            ))
            .unwrap();
        }
    }

    // Callback function that gets executed to reload csh configuration when the reload command is sent to the socket
    let reload_callback = Arc::new(|| {

        let new_config = load_config();
        {
            match new_config {
                Ok(config) => {
                    let mut config_write = CONFIG.write().unwrap();
                    *config_write = config;
                }
                Err(e) => {
                    eprintln!("Failed to reload configuration: {}", e);
                }
            }
        }

        let new_rules = {
            let config_read = CONFIG.read().unwrap();
            compile_rules(&*config_read)
        };

        {
            let mut rules_write = COMPILED_RULES.write().unwrap();
            *rules_write = new_rules;
        }

        // If debugging, log the compiled rules
        if DEBUG_MODE.load(Ordering::Relaxed) {
            log_debug("Compiled rules:").unwrap();
            for (i, (regex, color)) in COMPILED_RULES.read().unwrap().iter().enumerate() {
                log_debug(&format!(
                    "  Rule {}: regex = {:?}, color = {:?}",
                    i + 1,
                    regex,
                    color
                ))
                .unwrap();
            }
        }
    });

    #[cfg(unix)]
    unix_socket::start_socket_listener(move || reload_callback());

    #[cfg(windows)]
    windows_pipe::start_socket_listener(move || reload_callback());

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

    //Close and clean up Socket/Pipe
    #[cfg(unix)]
    {
        unix_socket::send_command("exit");
    }

    #[cfg(windows)]
    {
        windows_pipe::send_command("exit");
    }

    // Wait for the SSH process to finish and exit with the process's status code
    let status = child.wait()?;
    std::process::exit(status.code().unwrap_or(1));
    
}
