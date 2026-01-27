//! Configuration file loading and management
//!
//! Handles:
//! - Searching for config files in multiple locations
//! - Creating default configuration if none exists
//! - Parsing YAML configuration
//! - Compiling regex patterns from rules
//! - Hot-reloading configuration changes

use super::style::Config;
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
    pub fn new(profile: Option<String>) -> Result<Self, io::Error> {
        let config_path = Self::find_config_path(&profile)?;
        Ok(Self { config_path })
    }

    /// Find the configuration file in standard locations
    fn find_config_path(profile: &Option<String>) -> Result<PathBuf, io::Error> {
        log_debug!("Searching for configuration file...");
        let config_filename = match profile {
            Some(p) if !p.is_empty() => format!("{}.csh-config.yaml", p),
            _ => ".csh-config.yaml".to_string(),
        };

        // Check first possible location: ~/.csh/{profile}.csh-config.yaml
        if let Some(home_dir) = dirs::home_dir() {
            let csh_dir_path = home_dir.join(".csh").join(&config_filename);
            log_debug!("Checking: {:?}", csh_dir_path);
            if csh_dir_path.exists() {
                log_info!("Found config at: {:?}", csh_dir_path);
                return Ok(csh_dir_path);
            }
        }

        // Check second possible location: ~/{profile}.csh-config.yaml
        if let Some(home_dir) = dirs::home_dir() {
            let home_dir_path = home_dir.join(&config_filename);
            log_debug!("Checking: {:?}", home_dir_path);
            if home_dir_path.exists() {
                log_info!("Found config at: {:?}", home_dir_path);
                return Ok(home_dir_path);
            }
        }

        // Check third possible location: current working directory
        let current_dir_path = env::current_dir()
            .unwrap_or_else(|err| {
                eprintln!("Failed to get current directory: {}", err);
                std::process::exit(1);
            })
            .join(&config_filename);
        log_debug!("Checking: {:?}", current_dir_path);
        if current_dir_path.exists() {
            log_info!("Found config at: {:?}", current_dir_path);
            return Ok(current_dir_path);
        }

        // If a profile was specified but no file found, error out
        if profile.is_some() {
            let err_msg = format!(
                "Configuration profile '{}' not found. Please ensure the file exists in one of the standard locations.",
                profile.as_ref().unwrap()
            );
            log_warn!("{}", err_msg);
            return Err(io::Error::new(io::ErrorKind::NotFound, err_msg));
        }

        // No profile specified and no config files exist; try to create a default configuration.
        log_warn!("No config file found, creating default configuration");
        match Self::create_default_config() {
            Ok(path) => Ok(path),
            Err(err) => {
                eprintln!("Failed to create default configuration file: {}\r", err);
                Err(err)
            }
        }
    }

    /// Create a default configuration file if none exists
    fn create_default_config() -> io::Result<PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to get home directory"))?;
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
        let config_content = fs::read_to_string(self.config_path.clone()).map_err(|err| {
            log_warn!("Failed to read config file: {}", err);
            err
        })?;

        match serde_yaml::from_str::<Config>(&config_content) {
            Ok(mut config) => {
                config.metadata.config_path = self.config_path;
                log_debug!("Parsed configuration successfully");

                // Validate and convert hex colors to ANSI codes
                let mut invalid_colors = Vec::new();
                for (color_name, value) in config.palette.iter_mut() {
                    // Validate hex color format before conversion
                    if !is_valid_hex_color(value) {
                        log_warn!("Invalid hex color '{}' for palette entry '{}', using reset", value, color_name);
                        invalid_colors.push(color_name.clone());
                    }
                    let ansi_code = hex_to_ansi(value);
                    log_debug!("Converted color '{}': {} -> {}", color_name, value, ansi_code.escape_debug());
                    *value = ansi_code;
                }

                if !invalid_colors.is_empty() {
                    log_warn!("Found {} invalid color(s): {:?}", invalid_colors.len(), invalid_colors);
                }

                // Compile the rules
                let compiled_rules = compile_rules(&config);
                log_info!("Compiled {} highlight rules", compiled_rules.len());
                config.metadata.compiled_rules = compiled_rules;
                
                // Compile secret redaction patterns
                let compiled_secrets = compile_secret_patterns(&config);
                if !compiled_secrets.is_empty() {
                    log_info!("Compiled {} secret redaction patterns", compiled_secrets.len());
                }
                config.metadata.compiled_secret_patterns = compiled_secrets;
                
                Ok(config)
            }
            Err(err) => {
                log_warn!("Error parsing configuration file: {:?}", err);
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Error parsing configuration file: {:?}", err),
                ))
            }
        }
    }

    /// Loads and applies new configuration.
    pub fn reload_config(self) -> Result<(), String> {
        log_info!("Reloading configuration...");
        let mut current_config = super::get_config().write().unwrap();

        let mut new_config = self.load_config().map_err(|err| {
            log_warn!("Failed to reload configuration: {}", err);
            err.to_string()
        })?;

        // Preserve session name across reloads
        new_config.metadata.session_name = current_config.metadata.session_name.clone();
        // Increment version to signal config change to active threads
        new_config.metadata.version = current_config.metadata.version.wrapping_add(1);

        *current_config = new_config;

        let new_rules = compile_rules(&current_config);
        log_info!("Recompiled {} highlight rules", new_rules.len());

        current_config.metadata.compiled_rules = new_rules;
        
        // Recompile secret patterns
        let new_secrets = compile_secret_patterns(&current_config);
        if !new_secrets.is_empty() {
            log_info!("Recompiled {} secret redaction patterns", new_secrets.len());
        }
        current_config.metadata.compiled_secret_patterns = new_secrets;
        
        log_info!("Configuration reloaded successfully (version {})", current_config.metadata.version);

        Ok(())
    }
}

/// Validates that a color string is a valid hex color code
///
/// # Arguments
/// * `color` - The color string to validate
///
/// # Returns
/// `true` if the color is a valid hex code (#RRGGBB), `false` otherwise
fn is_valid_hex_color(color: &str) -> bool {
    if color.len() != 7 || !color.starts_with('#') {
        return false;
    }
    color[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding color
fn compile_rules(config: &Config) -> Vec<(Regex, String)> {
    let mut rules = Vec::new();
    let mut failed_rules = Vec::new();
    let mut missing_colors = Vec::new();

    for (idx, rule) in config.rules.iter().enumerate() {
        // Check if the referenced color exists in the palette
        let color = match config.palette.get(&rule.color) {
            Some(c) => c.clone(),
            None => {
                missing_colors.push((idx + 1, rule.color.clone()));
                "\x1b[0m".to_string() // Default to reset color if not found
            }
        };

        // This is done to make sure newline characters are removed from the string before they are loaded into a Regex value
        // This will not remove the string value "\n" just actually new line characters Ex. "Hello\nWorld" will not have "\n" replaced because it is the string "\n" instead of the actual newline character
        let clean_regex = rule.regex.replace('\n', "").trim().to_string();

        match Regex::new(&clean_regex) {
            Ok(regex) => rules.push((regex, color)),
            Err(err) => {
                eprintln!("Warning: Invalid regex '{}' - {}\r", clean_regex, err);
                failed_rules.push((idx + 1, clean_regex));
            }
        }
    }

    // Log validation summary
    if !missing_colors.is_empty() {
        log_warn!("Rules referencing missing palette colors: {:?}", missing_colors);
    }
    if !failed_rules.is_empty() {
        log_warn!("Failed to compile {} regex rule(s)", failed_rules.len());
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

/// Compiles secret redaction patterns from the configuration
///
/// - `config`: A reference to the Config struct containing secret patterns
///
/// Returns a vector of compiled Regex patterns for secret redaction
fn compile_secret_patterns(config: &Config) -> Vec<Regex> {
    let mut patterns = Vec::new();
    
    if let Some(secret_strings) = &config.settings.remove_secrets {
        for (idx, pattern) in secret_strings.iter().enumerate() {
            match Regex::new(pattern) {
                Ok(regex) => patterns.push(regex),
                Err(err) => {
                    log_warn!("Failed to compile secret pattern #{}: '{}' - {}", idx + 1, pattern, err);
                    eprintln!("Warning: Invalid secret pattern '{}' - {}\r", pattern, err);
                }
            }
        }
    }
    
    patterns
}
