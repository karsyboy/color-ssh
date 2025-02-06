use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

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

// Find the configuration path and set once as a global varible to be used anywhere
// This is done so we dont have to keep worrying about passing the config path to functions inorder for them to get config information when its reloaded
pub const CONFIG_PATH: Lazy<PathBuf> = Lazy::new(|| {
    // Check first possible location: ~/.csh/.csh-config.yaml
    if let Some(home_dir) = dirs::home_dir() {
        let csh_dir_path = home_dir.join(".csh").join(".csh-config.yaml");
        if csh_dir_path.exists() {
            return csh_dir_path;
        }
    }

    // Check second possible location: ~/.csh-config.yaml
    if let Some(home_dir) = dirs::home_dir() {
        let home_dir_path = home_dir.join(".csh-config.yaml");
        if home_dir_path.exists() {
            return home_dir_path;
        }
    }

    // Check third possible location: current working directory
    let current_dir_path = env::current_dir()
        .unwrap_or_else(|e| {
            eprintln!("Failed to get current directory: {}", e);
            std::process::exit(1);
        })
        .join(".csh-config.yaml");
    if current_dir_path.exists() {
        return current_dir_path;
    }

    // None of the config files exist; try to create a default configuration.
    match create_default_config() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to create default configuration file: {}", e);
            std::process::exit(1);
        }
    }
});

// Load initial config, and compiled rules as statics so that they can be updated and changed when the socket calls a reload this also alows them to be used globally
pub static CONFIG: Lazy<Arc<RwLock<Config>>> = Lazy::new(|| {
    Arc::new(RwLock::new(
        load_config().expect("Failed to load configuration."),
    ))
});

pub static COMPILED_RULES: Lazy<Arc<RwLock<Vec<(Regex, String)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(compile_rules(&*CONFIG.read().unwrap()))));

/// Reads the configuration file and parses it into a Config struct
/// Returns an io::Result containing the Config struct or an error
pub fn load_config() -> io::Result<Config> {
    if DEBUG_MODE.load(Ordering::Relaxed) {
        log_debug(&format!("Using configuration file: {:?}", CONFIG_PATH)).unwrap();
    }

    let config_content = fs::read_to_string(&*CONFIG_PATH)?;

    match serde_yaml::from_str::<Config>(&config_content) {
        Ok(mut config) => {
            // Convert hex color codes to ANSI escape sequences
            for (_, value) in config.palette.iter_mut() {
                *value = hex_to_ansi(value);
            }
            Ok(config)
        }
        Err(err) => {
            eprintln!("Error parsing configuration file: {:?}", err);
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Failed to parse configuration file.",
            ))
        }
    }
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding color
pub fn compile_rules(config: &Config) -> Vec<(Regex, String)> {
    let mut rules = Vec::new();

    for rule in &config.rules {
        let color = config
            .palette
            .get(&rule.color)
            .cloned()
            .unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color if not found

        match Regex::new(&rule.regex) {
            Ok(regex) => rules.push((regex, color)),
            Err(err) => eprintln!("Warning: Invalid regex '{}' - {}", rule.regex, err),
        }
    }

    rules
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
