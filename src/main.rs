use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{thread, time::Duration};
use std::sync::{Arc, RwLock};
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;

// Imports CSH specific modules
mod cli;
mod config;
mod highlighter;
mod logging;
mod ssh;
mod vault;

use cli::{parse_args, SSH_LOGGING};
use config::{compile_rules, load_config, find_config_file};
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

    // Load initial config path, config, and compiled rules for use in config watcher thread and rest of program
    let config_path = find_config_file().expect("Configuration file not found.");
    let config = Arc::new(RwLock::new(load_config(&config_path).expect("Failed to load configuration.")));
    let compiled_rules = Arc::new(RwLock::new(compile_rules(&config.read().unwrap())));

    // If debugging, log the compiled rules
    if DEBUG_MODE.load(Ordering::Relaxed) {
        let rules = compiled_rules.read().unwrap().clone();
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

    // Create the needed Arc clones for the file watcher thread
    let watch_config_path = config_path.clone();
    let watch_config = Arc::clone(&config);
    let watch_compiled_rules = Arc::clone(&compiled_rules);

    // Spawn a thread to watch the configuration file for changes
    // If changes are done to the configuration file it will reload the configuration and recompile the rules
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut config_watcher = RecommendedWatcher::new(
            move |result: Result<Event, Error>| {
                if let Ok(event) = result {
                    if event.kind.is_modify() {
                        tx.send(()).unwrap();
                    }
                }
            },
            notify::Config::default(),
        ).expect("Failed to initialize watcher");
    
        config_watcher.watch(Path::new(&watch_config_path), RecursiveMode::NonRecursive)
            .expect("Failed to watch configuration file");
    
        for _ in rx.iter() {
            println!("Config file changed, reloading...");
            thread::sleep(Duration::from_millis(500)); // Avoid race conditions

            match load_config(&watch_config_path) {
                Ok(new_config) => {
                    // Update the config first
                    {
                        let mut config_write = watch_config.write().unwrap();
                        *config_write = new_config;
                    }
    
                    // Then use the updated config to compile new rules
                    let new_rules = {
                        let config_read = watch_config.read().unwrap();
                        compile_rules(&config_read)
                    };

                    println!("Configuration reloaded successfully.");

                    // If debugging, log the compiled rules
                    if DEBUG_MODE.load(Ordering::Relaxed) {
                        log_debug("Compiled rules:").unwrap();
                        for (i, (regex, color)) in new_rules.iter().enumerate() {
                            log_debug(&format!(
                                "  Rule {}: regex = {:?}, color = {:?}",
                                i + 1,
                                regex,
                                color
                            ))
                            .unwrap();
                        }
                    }

                    // Update the compiled rules
                    *watch_compiled_rules.write().unwrap() = new_rules;
                }
                Err(error) => {
                    eprintln!("Error reloading config: {:?}. Retaining old configuration.", error);
                }
            }
        }
    });

    // Launch the SSH process with the provided arguments
    let mut child = spawn_ssh(&args)?;
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();


    // Create needed values for chunk reading thread
    let proc_reset_color = "\x1b[0m"; // ANSI reset color sequence
    let proc_compiled_rules = Arc::clone(&compiled_rules);

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
