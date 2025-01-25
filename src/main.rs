use std::env;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    palette: std::collections::HashMap<String, String>, // Color palette (hex colors)
    rules: Vec<HighlightRule>,                         // Highlighting rules
}

#[derive(Debug, Deserialize)]
struct HighlightRule {
    regex: String,       // Multi-line regex for matching text
    color: String,       // Color key from the palette
}

// Function to convert a hex color (e.g., "#e67549") to an ANSI escape code.
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
    "\x1b[0m".to_string() // Default to reset color if hex is invalid.
}

fn find_config_file() -> Option<PathBuf> {
    // Check current directory
    let current_dir_path = env::current_dir().unwrap().join(".csh-config.yaml");
    if current_dir_path.exists() {
        return Some(current_dir_path);
    }

    // Check home directory
    if let Some(home_dir) = dirs::home_dir() {
        let home_dir_path = home_dir.join(".csh-config.yaml");
        if home_dir_path.exists() {
            return Some(home_dir_path);
        }
    }

    None
}

fn main() -> io::Result<()> {
    // Get the command-line arguments (excluding the program name).
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("Usage: csh <ssh arguments>");
        std::process::exit(1);
    }

    // Read the YAML configuration file.
    let config_path: PathBuf = find_config_file()
        .expect("Configuration file not found.");
    let config_content = fs::read_to_string(config_path)
        .expect("Failed to read the configuration file.");
    let config: Config = serde_yaml::from_str(&config_content)
        .expect("Failed to parse the configuration file.");

    // Compile regex patterns and map them to their corresponding ANSI escape codes.
    let rules: Vec<(Regex, String)> = config
        .rules
        .iter()
        .map(|rule| {
            let color = config
                .palette
                .get(&rule.color)
                .map(|hex| hex_to_ansi(hex))
                .unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color.
            let regex = Regex::new(&rule.regex)
                .expect("Invalid regex in configuration.");
            (regex, color)
        })
        .collect();

    // Spawn the SSH process.
    let mut child = Command::new("ssh")
        .args(&args)
        .stdin(Stdio::inherit()) // Pass stdin to the child process.
        .stdout(Stdio::piped())  // Capture stdout.
        .stderr(Stdio::inherit()) // Pass stderr to the child process.
        .spawn()?;

    // Create a BufReader for the child's stdout.
    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut reader = BufReader::new(stdout);

    // Get the current process's stdout for writing modified output.
    let mut stdout = io::stdout();

    // Buffer for reading chunks of data.
    let mut buffer = [0; 1024];
    let reset_color = "\x1b[0m"; // ANSI escape code to reset color.

    // Process the output in chunks to preserve interactivity.
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break; // EOF reached.
        }

        let chunk = String::from_utf8_lossy(&buffer[..n]);
        let mut matches: Vec<(usize, usize, String)> = Vec::new(); // Start, end, color for matches

        // Collect matches for all rules
        for (regex, color) in &rules {
            for caps in regex.captures_iter(&chunk) {
                if let Some(m) = caps.get(0) {
                    matches.push((m.start(), m.end(), color.clone()));
                }
            }
        }

        // Sort matches by start position
        matches.sort_by_key(|&(start, _, _)| start);

        // Handle overlaps by precedence (earlier rules in the list take precedence)
        let mut filtered_matches: Vec<(usize, usize, String)> = Vec::new();
        for &(start, end, ref color) in &matches {
            if filtered_matches
                .iter()
                .all(|&(s, e, _)| end <= s || start >= e)
            {
                filtered_matches.push((start, end, color.clone()));
            }
        }

        // Reconstruct the processed chunk with highlights applied.
        let mut processed_chunk = String::new();
        let mut last_idx = 0;

        for (start, end, color) in filtered_matches {
            if start > last_idx {
                processed_chunk.push_str(&chunk[last_idx..start]); // Add uncolored text.
            }
            processed_chunk.push_str(&format!(
                "{}{}{}",
                color,
                &chunk[start..end],
                reset_color
            )); // Add colored text.
            last_idx = end;
        }

        if last_idx < chunk.len() {
            processed_chunk.push_str(&chunk[last_idx..]); // Add any remaining uncolored text.
        }

        // Write the fully processed chunk to stdout.
        stdout.write_all(processed_chunk.as_bytes())?;
        stdout.flush()?;
    }

    // Wait for the SSH process to finish.
    let status = child.wait()?;

    // Exit with the same status code as the SSH process.
    std::process::exit(status.code().unwrap_or(1));
}
