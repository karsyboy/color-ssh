use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::{log_debug, DEBUG_MODE};

// Configuration structure containing the color palette and highlighting rules
#[derive(Debug, Deserialize)]
pub struct Config {
    pub palette: HashMap<String, String>, // Map of color names (keys) to their respective hex codes (values)
    pub rules: Vec<HighlightRule>, // List of highlighting rules with a regex pattern and the corresponding color
}

// Structure representing a single highlight rule
#[derive(Debug, Deserialize)]
pub struct HighlightRule {
    pub regex: String, // Regex pattern to match text for highlighting
    pub color: String, // Color name (key in the palette) to use for the matched text
}

/// Reads the configuration file and parses it into a Config struct
/// Returns an io::Result containing the Config struct or an error
pub fn load_config() -> io::Result<Config> {
    let config_path = find_config_file().expect("Configuration file not found.");

    if DEBUG_MODE.load(Ordering::Relaxed) {
        log_debug(&format!("Using configuration file: {:?}", config_path)).unwrap();
    }

    let config_content = fs::read_to_string(config_path)?;
    let mut config: Config =
        serde_yaml::from_str(&config_content).expect("Failed to parse the configuration file.");

    // Convert hex color codes to ANSI escape sequences
    for (_, value) in config.palette.iter_mut() {
        *value = hex_to_ansi(value);
    }

    Ok(config)
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding color
pub fn compile_rules(config: &Config) -> Vec<(Regex, String)> {
    let rules: Vec<(Regex, String)> = config
        .rules
        .iter()
        .map(|rule| {
            // Retrieve the already-converted ANSI escape sequence from the palette.
            let color = config
                .palette
                .get(&rule.color)
                .cloned()
                .unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color if not found

            // Compile the regex pattern for matching
            let regex = Regex::new(&rule.regex).expect("Invalid regex in configuration.");
            (regex, color)
        })
        .collect();
    rules
}

/// Search for the configuration file in the current directory or home directory
/// Returns the path to the configuration file if found, or None if not found
fn find_config_file() -> Option<PathBuf> {
    // look for .csh-config.yaml in the .csh directory under the users home directory
    if let Some(home_dir) = dirs::home_dir() {
        let csh_dir_path = home_dir.join(".csh").join(".csh-config.yaml");
        if csh_dir_path.exists() {
            return Some(csh_dir_path);
        }
    }

    // Look for .csh-config.yaml in the home directory if not found in the current directory
    if let Some(home_dir) = dirs::home_dir() {
        let home_dir_path = home_dir.join(".csh-config.yaml");
        if home_dir_path.exists() {
            return Some(home_dir_path);
        }
    }

    // Look for .csh-config.yaml in the current directory
    let current_dir_path = env::current_dir().unwrap().join(".csh-config.yaml");
    if current_dir_path.exists() {
        return Some(current_dir_path);
    }

    // Create a default configuration file if no configuration file is found
    match create_default_config() {
        Ok(path) => Some(path),
        Err(e) => {
            eprintln!("Failed to create default configuration file: {}", e);
            None
        }
    }

}

/// Converts a hex color code (e.g., "#FFFFFF") to an ANSI escape sequence for terminal color
///
/// - `hex`: A string slice representing the hex color code
///
/// Returns a string containing the ANSI escape sequence for the RGB color, or a reset sequence if the hex code is invalid
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

/// Creates a default configuration file with sample content if config does not exist in home directory .csh
/// Returns an io::Result containing the path to the created configuration file or an error
fn create_default_config() -> io::Result<PathBuf> {
    let home_dir = dirs::home_dir().expect("Failed to get home directory.");
    let csh_dir = home_dir.join(".csh");
    let config_path = csh_dir.join(".csh-config.yaml");

    // Create the .csh directory if it does not exist
    if !csh_dir.exists() {
        fs::create_dir(&csh_dir)?;
    }

    // Create the configuration file with sample content
    let config_content = r#"# Description: This is the default template created by color-ssh (csh). 
# It contains information on the template layout and how to create a custom template.
# color-ssh templates can be found at https://github.com/karsyboy/color-ssh

# The palette section is used to define the colors that can be used in the rules section.
# The colors are defined in hex format.
palette:
  Red: '#c71800'
  Green: '#28c501'
  Blue: '#5698c8'

rules:
# example rule with all possible options
# - description: Match on the word "example"
#   regex: |
#     (?ix)
#       \b
#       example
#       \b
#   color: Kelly-Green
# create a rule that matches on the word "connected" or "up" and color it Kelly-Green

# Example of a rule that uses a one line regex to match on "good" or "up" and color it Green
- description: Match on good keywords
  regex: (?ix)\b(good|up)\b
  color: Green


- description: Match on neutral keywords
  regex: |
    (?ix)
      \b
      neutral
      \b
  color: Blue

# create a rule that matches on the word "down" or "error" or "disabled" and color it Red
- description: Match on bad keywords
  regex: |
    (?ix)
      \b
      (down|error|disabled)
      \b
  color: Red"#;
    fs::write(&config_path, config_content)?;

    Ok(config_path)
}