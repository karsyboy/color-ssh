use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use regex::Regex;
use serde::Deserialize;

// Configuration structure containing the color palette and highlighting rules
#[derive(Debug, Deserialize)]
struct Config {
    palette: std::collections::HashMap<String, String>, // Color palette (hex colors mapped to names)
    rules: Vec<HighlightRule>,                         // Highlighting rules for specific patterns
}

// Structure representing a single highlight rule
#[derive(Debug, Deserialize)]
struct HighlightRule {
    regex: String,       // Regex pattern to match text
    color: String,       // Color name (key in the palette) for matched text
}

// Converts a hex color code (e.g., "#FFFFFF") to an ANSI escape sequence for terminal color
fn hex_to_ansi(hex: &str) -> String {
    if hex.len() == 7 && hex.starts_with('#') {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[1..3], 16),
            u8::from_str_radix(&hex[3..5], 16),
            u8::from_str_radix(&hex[5..7], 16),
        ) {
            return format!("\x1b[38;2;{};{};{}m", r, g, b);
        }
    }
    "\x1b[0m".to_string() // Default to reset color if parsing fails
}

// Created a debug log in the $PWD directory and outputs debug logs to it if --dubug is used
fn log_debug(message: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("csh-debug.log")?;
    writeln!(file, "{}", message)
}

// Locates the configuration file, searching in the current directory and the home directory
fn find_config_file() -> Option<PathBuf> {
    // Check for .csh-config.yaml in the current directory
    let current_dir_path = env::current_dir().unwrap().join(".csh-config.yaml");
    if current_dir_path.exists() {
        return Some(current_dir_path);
    }

    // Check for .csh-config.yaml in the user's home directory
    if let Some(home_dir) = dirs::home_dir() {
        let home_dir_path = home_dir.join(".csh-config.yaml");
        if home_dir_path.exists() {
            return Some(home_dir_path);
        }
    }

    None // Configuration file not found
}

// Processes a chunk of text, applying syntax highlighting based on the provided rules
fn process_chunk(chunk: String, rules: &[(Regex, String)], reset_color: &str) -> String {

    let mut matches: Vec<(usize, usize, String)> = Vec::new();

    // Match each rule's regex against the input chunk
    for (regex, color) in rules {
        for caps in regex.captures_iter(&chunk) {
            if let Some(m) = caps.get(0) {
                matches.push((m.start(), m.end(), color.clone()));
            }
        }
    }

    // Sort matches by start position for sequential processing
    matches.sort_by_key(|&(start, _, _)| start);

    // Filter overlapping matches to keep only non-overlapping ones
    let mut filtered_matches = Vec::new();
    for &(start, end, ref color) in &matches {
        if filtered_matches.iter().all(|&(s, e, _)| end <= s || start >= e) {
            filtered_matches.push((start, end, color.clone()));
        }
    }

    // Build the highlighted output string
    let mut processed_chunk = String::new();
    let mut last_idx = 0;

    for (start, end, color) in filtered_matches {
        if start > last_idx {
            processed_chunk.push_str(&chunk[last_idx..start]);
        }
        processed_chunk.push_str(&format!("{}{}{}", color, &chunk[start..end], reset_color));
        last_idx = end;
    }

    if last_idx < chunk.len() {
        processed_chunk.push_str(&chunk[last_idx..]);
    }

    processed_chunk
}

// Entry point of the program
fn main() -> io::Result<()> {
    // SSH arguments for debugging (replace as needed for actual usage)
    // let debug_args: Vec<String> = vec!["admin@10.64.15.254".to_string()];

    // Get the command-line arguments (excluding the program name). 
    let mut args: Vec<String> = env::args().skip(1).collect();
    // Remove the debug flag from args if present
    if args.is_empty() {
        eprintln!("Usage: csh <ssh arguments>");
        std::process::exit(1);
    }

    // Check if the debug flag is present
    let debug_mode = args.contains(&"--debug".to_string());
    // Remove the debug flag from args if present
    if debug_mode {
        args.retain(|arg| arg != "--debug");
        log_debug("Debug mode enabled").unwrap();
    }

    // Debug: Print SSH arguments for validation
    if debug_mode {
        log_debug(&format!("SSH arguments: {:?}", args)).unwrap();
    }

    // Load the configuration file
    let config_path = find_config_file().expect("Configuration file not found.");
    if debug_mode {
        log_debug(&format!("Using configuration file: {:?}", config_path)).unwrap();
    }
    let config_content = fs::read_to_string(config_path).expect("Failed to read the configuration file.");
    let config: Config = serde_yaml::from_str(&config_content).expect("Failed to parse the configuration file.");

    // Compile regex rules and map colors
    let rules: Vec<(Regex, String)> = config
        .rules
        .iter()
        .map(|rule| {
            let color = config
                .palette
                .get(&rule.color)
                .map(|hex| hex_to_ansi(hex))
                .unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color

            // Normalize the regex to remove newlines and unnecessary whitespace
            let normalized_regex = rule.regex.replace("\n  ", "").trim().to_string();
            if debug_mode {
                log_debug(&format!("Compiling regex: {:?}", normalized_regex)).unwrap();
            }
            let regex = Regex::new(&normalized_regex).expect("Invalid regex in configuration.");

            (regex, color)
        })
        .collect();

    // Debug: List compiled rules
if debug_mode {
    log_debug("Compiled rules:").unwrap();
    for (i, (regex, color)) in rules.iter().enumerate() {
        log_debug(&format!("  Rule {}: regex = {:?}, color = {:?}", i + 1, regex, color)).unwrap();
    }
}

    // Launch the SSH process
    let mut child = Command::new("ssh")
        .args(&args)
        // .args(&debug_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Handle SSH output
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving chunks of output
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let reset_color = "\x1b[0m";

    // Spawn a thread for processing and displaying highlighted output
    let rules_clone = rules.clone();
    thread::spawn(move || {
        while let Ok(chunk) = rx.recv() {
            let processed_chunk = process_chunk(chunk, &rules_clone, reset_color);
            print!("{}", processed_chunk);
            io::stdout().flush().unwrap();
        }
    });

    // Read data from the SSH process and send it for processing
    let mut buffer = [0; 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break; // Exit loop when EOF is reached
        }
        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
        if debug_mode {
            log_debug(&format!("Read Chunk: {:?}", chunk)).unwrap();
        }
        tx.send(chunk).expect("Failed to send data to processing thread");
    }

    // Wait for the SSH process to finish
    let status = child.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}
