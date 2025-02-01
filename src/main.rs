use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use regex::Regex;
use serde::Deserialize;

// Flag for enabling/disabling debug mode
static DEBUG_MODE: bool = false;

// Configuration structure containing the color palette and highlighting rules
#[derive(Debug, Deserialize)]
struct Config {
    // Map of color names (keys) to their respective hex codes (values)
    palette: std::collections::HashMap<String, String>,
    
    // List of highlighting rules with a regex pattern and the corresponding color
    rules: Vec<HighlightRule>,
}

// Structure representing a single highlight rule
#[derive(Debug, Deserialize)]
struct HighlightRule {
    regex: String,  // Regex pattern to match text for highlighting
    color: String,  // Color name (key in the palette) to use for the matched text
}

// Converts a hex color code (e.g., "#FFFFFF") to an ANSI escape sequence for terminal color
fn hex_to_ansi(hex: &str) -> String {
    // Check if the hex code is valid (starts with '#' and has 7 characters)
    if hex.len() == 7 && hex.starts_with('#') {
        // Parse the red, green, and blue values from the hex string
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[1..3], 16),
            u8::from_str_radix(&hex[3..5], 16),
            u8::from_str_radix(&hex[5..7], 16),
        ) {
            // Return the ANSI escape sequence for the RGB color
            return format!("\x1b[38;2;{};{};{}m", r, g, b);
        }
    }
    // Return the reset color sequence if the hex is invalid
    "\x1b[0m".to_string()
}

// Log messages to a debug log file, if DEBUG_MODE is enabled
fn log_debug(message: &str) -> io::Result<()> {
    // Open (or create) a file for appending log messages
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("csh-debug.log")?;
    // Write the message to the log file
    writeln!(file, "{}", message)
}

// Search for the configuration file in the current directory or home directory
fn find_config_file() -> Option<PathBuf> {
    // Look for .csh-config.yaml in the current directory
    let current_dir_path = env::current_dir().unwrap().join(".csh-config.yaml");
    if current_dir_path.exists() {
        return Some(current_dir_path);
    }

    // Look for .csh-config.yaml in the home directory if not found in the current directory
    if let Some(home_dir) = dirs::home_dir() {
        let home_dir_path = home_dir.join(".csh-config.yaml");
        if home_dir_path.exists() {
            return Some(home_dir_path);
        }
    }

    // Return None if the config file was not found
    None
}

// Build a cleaned-up version of the input string with a mapping to the original indices
fn build_index_mapping(raw: &str) -> (String, Vec<usize>) {
    let mut clean = String::with_capacity(raw.len());
    // This will store the mapping between the cleaned string's indices and the raw string's indices
    let mut mapping = Vec::with_capacity(raw.len());

    let mut raw_idx = 0;
    for ch in raw.chars() {
        // Replace newline and carriage return characters with a space
        if ch == '\n' || ch == '\r' {
            clean.push(' ');
        } else {
            clean.push(ch);
        }
        mapping.push(raw_idx);
        raw_idx += ch.len_utf8(); // Keep track of the character's byte length
    }
    (clean, mapping)
}

// Processes a chunk of text, applying syntax highlighting based on the provided rules
fn process_chunk(chunk: String, rules: &[(Regex, String)], reset_color: &str) -> String {
    // Clean up the chunk and build the index mapping
    let (clean_chunk, mapping) = build_index_mapping(&chunk);

    let mut matches: Vec<(usize, usize, String, String)> = Vec::new();
    // Find all matches in the chunk using the provided regex rules
    for (regex, color) in rules {
        for mat in regex.find_iter(&clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            // Ensure clean_start and clean_end are within bounds before using them
            let raw_start = if clean_start < mapping.len() {
                mapping[clean_start]
            } else {
                0 // Fallback to 0 if clean_start is out of bounds
            };

            let raw_end = if clean_end < mapping.len() {
                mapping[clean_end]
            } else {
                chunk.len() // Fallback to the full length of the chunk if clean_end is out of bounds
            };
            // println!("clean_start: {}, clean_end: {}\n\r", clean_start, clean_end);
            // println!("raw_start: {}, raw_end: {}\n\r", raw_start, raw_end);

            // Extract the matched text and store it with the color
            let matched_text = chunk[raw_start..raw_end].to_owned();
            matches.push((raw_start, raw_end, matched_text, color.clone()));
        }
    }

    // Filter the matches to avoid overlap (keep only the most specific matches)
    matches.sort_by_key(|&(start, _, _, _)| start);
    let mut filtered_matches = matches.clone();
    // filtered_matches.retain(|&(s_start, s_end, _, _)| {
    //     !matches.iter().any(|&(other_start, other_end, _, _)| {
    //         (other_start <= s_start && other_end >= s_end) &&
    //         ((other_end - other_start) > (s_end - s_start))
    //     })
    // });



    // Sort the matches by their starting position in the raw chunk
    filtered_matches.sort_by_key(|&(start, _, _, _)| start);
    
    let mut highlighted = String::with_capacity(chunk.len());
    let mut last_index = 0;
    
    if DEBUG_MODE {
        log_debug(&format!("Raw chunk: {:?}", chunk)).unwrap();
        log_debug(&format!("Clean chunk: {:?}", clean_chunk)).unwrap();
        log_debug(&format!("Matches: {:?}", matches)).unwrap();
        log_debug(&format!("Filtered matches: {:?}", filtered_matches)).unwrap();
    }
    // Apply the color formatting to the chunk based on the matches
    for (start, end, matched_text, color) in filtered_matches {
        // Append the text between the last match and the current match
        // println!("last_index: {}, start: {}\n\r", last_index, start);
        if last_index > start {
            continue; // Skip if the last index is already at or beyond the start   
        }
        highlighted.push_str(&chunk[last_index..start]);
        // Append the matched text with color formatting
        highlighted.push_str(&format!("{}{}{}", color, matched_text, reset_color));
        last_index = end;
    }

    // Append the remaining text after the last match
    highlighted.push_str(&chunk[last_index..]);
    highlighted
}

fn main() -> io::Result<()> {
    // Get the command-line arguments, skipping the program name
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: csh <ssh arguments>");
        std::process::exit(1); // Exit if no arguments are provided
    }

    // If debugging, log the provided SSH arguments for verification
    if DEBUG_MODE {
        log_debug("Debug mode enabled").unwrap();
        log_debug(&format!("SSH arguments: {:?}", args)).unwrap();
    }

    // Search for the configuration file
    let config_path = find_config_file().expect("Configuration file not found.");
    if DEBUG_MODE {
        log_debug(&format!("Using configuration file: {:?}", config_path)).unwrap();
    }

    // Read the configuration file and parse it
    let config_content = fs::read_to_string(config_path).expect("Failed to read the configuration file.");
    let config: Config = serde_yaml::from_str(&config_content).expect("Failed to parse the configuration file.");

    // Compile regex rules and associate them with the corresponding colors
    let rules: Vec<(Regex, String)> = config
        .rules
        .iter()
        .map(|rule| {
            // Convert hex color to ANSI escape sequence
            let color = config
                .palette
                .get(&rule.color)
                .map(|hex| hex_to_ansi(hex))
                .unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color if not found
            // Compile the regex pattern for matching
            let regex = Regex::new(&rule.regex).expect("Invalid regex in configuration.");
            (regex, color)
        })
        .collect();

    // If debugging, log the compiled rules
    if DEBUG_MODE {
        log_debug("Compiled rules:").unwrap();
        for (i, (regex, color)) in rules.iter().enumerate() {
            log_debug(&format!("  Rule {}: regex = {:?}, color = {:?}", i + 1, regex, color)).unwrap();
        }
    }

    // Launch the SSH process with the provided arguments
    let mut child = Command::new("ssh")
        .args(&args)
        .stdin(Stdio::inherit())  // Inherit the input from the current terminal
        .stdout(Stdio::piped())   // Pipe the output for processing
        .stderr(Stdio::inherit()) // Inherit the error stream from the SSH process
        .spawn()?;

    // Capture the SSH output
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let reset_color = "\x1b[0m"; // ANSI reset color sequence

    // Clone the rules for use in the processing thread
    let rules_clone = rules.clone();
    thread::spawn(move || {
        while let Ok(chunk) = rx.recv() {
            // Process the chunk and apply highlighting
            let processed_chunk = process_chunk(chunk, &rules_clone, reset_color);
            print!("{}", processed_chunk); // Print the processed chunk
            io::stdout().flush().unwrap();  // Flush to ensure immediate display
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
        if DEBUG_MODE {
            log_debug(&format!("Read chunk: {:?}", chunk)).unwrap();
        }
        tx.send(chunk).expect("Failed to send data to processing thread");
    }

    // Wait for the SSH process to finish and exit with the process's status code
    let status = child.wait()?;
    std::process::exit(status.code().unwrap_or(1));
}
