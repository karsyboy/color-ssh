//! Configuration file loading and management
//!
//! Handles:
//! - Searching for config files in multiple locations
//! - Creating default configuration if none exists
//! - Parsing YAML configuration
//! - Compiling regex patterns from rules
//! - Hot-reloading configuration changes

use super::{SESSION_CONFIG, style::Config};
use crate::{debug_enabled, log_debug, log_info, log_warn};
use regex::Regex;
use std::{
    path::PathBuf,
    {env, fs, io},
};

pub struct ConfigLoader {
    config_path: PathBuf,
}

impl ConfigLoader {
    pub fn new() -> Self {
        let config_path = Self::find_config_path();
        Self { config_path }
    }

    /// Find the configuration file in standard locations
    fn find_config_path() -> PathBuf {
        log_debug!("Searching for configuration file...");
        
        // Check first possible location: ~/.csh/.csh-config.yaml
        if let Some(home_dir) = dirs::home_dir() {
            let csh_dir_path = home_dir.join(".csh").join(".csh-config.yaml");
            log_debug!("Checking: {:?}", csh_dir_path);
            if csh_dir_path.exists() {
                log_info!("Found config at: {:?}", csh_dir_path);
                return csh_dir_path;
            }
        }

        // Check second possible location: ~/.csh-config.yaml
        if let Some(home_dir) = dirs::home_dir() {
            let home_dir_path = home_dir.join(".csh-config.yaml");
            log_debug!("Checking: {:?}", home_dir_path);
            if home_dir_path.exists() {
                log_info!("Found config at: {:?}", home_dir_path);
                return home_dir_path;
            }
        }

        // Check third possible location: current working directory
        let current_dir_path = env::current_dir()
            .unwrap_or_else(|err| {
                eprintln!("Failed to get current directory: {}", err);
                std::process::exit(1);
            })
            .join(".csh-config.yaml");
        log_debug!("Checking: {:?}", current_dir_path);
        if current_dir_path.exists() {
            log_info!("Found config at: {:?}", current_dir_path);
            return current_dir_path;
        }

        // None of the config files exist; try to create a default configuration.
        log_warn!("No config file found, creating default configuration");
        match Self::create_default_config() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("Failed to create default configuration file: {}\r", err);
                std::process::exit(1);
            }
        }
    }

    /// Create a default configuration file if none exists
    fn create_default_config() -> io::Result<PathBuf> {
        let home_dir = dirs::home_dir().expect("Failed to get home directory.\r");
        let csh_dir = home_dir.join(".csh");
        let config_path = csh_dir.join(".csh-config.yaml");

        // Create the .csh directory if it does not exist
        if !csh_dir.exists() {
            log_debug!("Creating directory: {:?}", csh_dir);
            fs::create_dir(&csh_dir)?;
        }

        // Create the configuration file with sample content
        let config_content = include_str!("../../templates/default.csh-config.yaml");
        fs::write(&config_path, config_content)?;
        log_info!("Default configuration file created at: {:?}", config_path);

        Ok(config_path)
    }

    /// Load the configuration from the config file
    pub fn load_config(self) -> io::Result<Config> {
        log_info!("Loading configuration from: {:?}", self.config_path);

        // Read the configuration file
        let config_content = fs::read_to_string(self.config_path.clone())
            .map_err(|e| {
                log_warn!("Failed to read config file: {}", e);
                e
            })?;
        
        match serde_yaml::from_str::<Config>(&config_content) {
            Ok(mut config) => {
                config.metadata.config_path = self.config_path;
                log_debug!("Parsed configuration successfully");
                
                // Convert hex colors to ANSI codes
                for (color_name, value) in config.palette.iter_mut() {
                    let ansi_code = hex_to_ansi(value);
                    log_debug!("Converted color '{}': {} -> {}", color_name, value, ansi_code.escape_debug());
                    *value = ansi_code;
                }
                
                // Compile the rules
                let compiled_rules = compile_rules(&config);
                log_info!("Compiled {} highlight rules", compiled_rules.len());
                config.metadata.compiled_rules = compiled_rules;
                Ok(config)
            }
            Err(err) => {
                log_warn!("Error parsing configuration file: {:?}", err);
                eprintln!("Error parsing configuration file: {:?}\r", err);
                Err(io::Error::new(io::ErrorKind::InvalidData, "Failed to parse configuration file.\r"))
            }
        }
    }

    /// Loads and applies new configuration.
    pub fn reload_config(self) -> Result<(), String> {
        log_info!("Reloading configuration...");
        let mut current_config = SESSION_CONFIG.write().unwrap();

        let mut new_config = self.load_config()
            .map_err(|err| {
                log_warn!("Failed to reload configuration: {}", err);
                format!("Failed to load configuration: {}\r", err)
            })?;

        // Preserve session name across reloads
        new_config.metadata.session_name = current_config.metadata.session_name.clone();

        *current_config = new_config;

        let new_rules = compile_rules(&*current_config);
        log_info!("Recompiled {} highlight rules", new_rules.len());

        current_config.metadata.compiled_rules = new_rules;
        log_info!("Configuration reloaded successfully");

        Ok(())
    }
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding color
fn compile_rules(config: &Config) -> Vec<(Regex, String)> {
    let mut rules = Vec::new();

    for rule in &config.rules {
        let color = config.palette.get(&rule.color).cloned().unwrap_or_else(|| "\x1b[0m".to_string()); // Default to reset color if not found

        // This is done to make sure newline characters are removed form the string before they are loaded into a Regex value
        // This will not remove the string value "\n" just actually new line characters Ex. "Hello\nWorld" will not have "\n" replaced because it is the string "\n" instead of the actual newline character
        let clean_regex = rule.regex.replace('\n', "").trim().to_string();

        match Regex::new(&clean_regex) {
            Ok(regex) => rules.push((regex, color)),
            Err(err) => eprintln!("Warning: Invalid regex '{}' - {}\r", clean_regex, err),
        }
    }

    if debug_enabled!() {
        for (i, (regex, color)) in rules.iter().enumerate() {
            log_debug!("Rule {}: regex = {:?}, color = {:?}", i + 1, regex, color);
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
