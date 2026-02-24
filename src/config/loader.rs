//! Configuration file loading and management
//!
//! Handles:
//! - Searching for config files in multiple locations
//! - Creating default configuration if none exists
//! - Parsing YAML configuration
//! - Compiling regex patterns from rules
//! - Hot-reloading configuration changes

use super::style::Config;
use crate::highlighter::CompiledHighlightRule;
use crate::{debug_enabled, log_debug, log_error, log_info, log_warn};
use regex::{Regex, RegexSet};
use std::{
    path::PathBuf,
    {env, fs, io},
};

const DEFAULT_CONFIG_FILENAME: &str = "cossh-config.yaml";

fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

pub(crate) struct ConfigLoader {
    config_path: PathBuf,
}

impl ConfigLoader {
    // Construction / path discovery.
    pub(crate) fn new(profile: Option<String>) -> Result<Self, io::Error> {
        let config_path = Self::find_config_path(&profile)?;
        Ok(Self { config_path })
    }

    /// Find the configuration file in standard locations
    fn find_config_path(profile: &Option<String>) -> Result<PathBuf, io::Error> {
        log_debug!("Searching for configuration file...");
        let normalized_profile = match profile.as_deref().map(str::trim) {
            Some(profile_name) if !profile_name.is_empty() => {
                if !is_valid_profile_name(profile_name) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Invalid profile name '{}': use only letters, numbers, '_' or '-'", profile_name),
                    ));
                }
                Some(profile_name.to_string())
            }
            _ => None,
        };
        let config_filename = match &normalized_profile {
            Some(profile_name) => format!("{}.cossh-config.yaml", profile_name),
            None => DEFAULT_CONFIG_FILENAME.to_string(),
        };

        // Check first possible location: ~/.color-ssh/{config_filename}
        if let Some(home_dir) = dirs::home_dir() {
            let cossh_dir_path = home_dir.join(".color-ssh").join(&config_filename);
            log_debug!("Checking: {:?}", cossh_dir_path);
            if cossh_dir_path.exists() {
                log_info!("Found config at: {:?}", cossh_dir_path);
                return Ok(cossh_dir_path);
            }
        }

        // Check second possible location: ~/{config_filename}
        if let Some(home_dir) = dirs::home_dir() {
            let home_dir_path = home_dir.join(&config_filename);
            log_debug!("Checking: {:?}", home_dir_path);
            if home_dir_path.exists() {
                log_info!("Found config at: {:?}", home_dir_path);
                return Ok(home_dir_path);
            }
        }

        // Check third possible location: current working directory
        let current_dir = env::current_dir().map_err(|err| {
            log_warn!("Failed to get current directory: {}", err);
            io::Error::new(io::ErrorKind::NotFound, format!("Failed to get current directory: {}", err))
        })?;
        let current_dir_path = current_dir.join(&config_filename);
        log_debug!("Checking: {:?}", current_dir_path);
        if current_dir_path.exists() {
            log_info!("Found config at: {:?}", current_dir_path);
            return Ok(current_dir_path);
        }

        // If a profile was specified but no file found, error out
        if let Some(profile_name) = normalized_profile {
            let err_msg = format!(
                "Configuration profile '{}' not found. Please ensure the file exists in one of the standard locations.",
                profile_name
            );
            log_warn!("{}", err_msg);
            return Err(io::Error::new(io::ErrorKind::NotFound, err_msg));
        }

        // No profile specified and no config files exist; try to create a default configuration.
        log_warn!("No config file found, creating default configuration");
        match Self::create_default_config() {
            Ok(path) => Ok(path),
            Err(err) => {
                log_error!("Failed to create default configuration file: {}", err);
                Err(err)
            }
        }
    }

    /// Create a default configuration file if none exists
    fn create_default_config() -> io::Result<PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to get home directory"))?;
        let cossh_dir = home_dir.join(".color-ssh");
        let config_path = cossh_dir.join(DEFAULT_CONFIG_FILENAME);

        // Create the .cossh directory if it does not exist
        if !cossh_dir.exists() {
            log_debug!("Creating directory: {:?}", cossh_dir);
            fs::create_dir(&cossh_dir)?;
        }

        // Create the configuration file with sample content
        let config_content = include_str!("../../templates/default.cossh-config.yaml");
        fs::write(&config_path, config_content)?;
        log_info!("Default configuration file created at: {:?}", config_path);

        Ok(config_path)
    }

    // Initial load and compile pipeline.
    /// Load the configuration from the config file
    pub(crate) fn load_config(self) -> io::Result<Config> {
        log_info!("Loading configuration from: {:?}", self.config_path);

        // Read the configuration file
        let config_content = fs::read_to_string(self.config_path.clone()).map_err(|err| {
            log_warn!("Failed to read config file: {}", err);
            err
        })?;

        match serde_yml::from_str::<Config>(&config_content) {
            Ok(mut config) => {
                config.metadata.config_path = self.config_path;
                log_debug!("Parsed configuration successfully");

                // Validate hex colors but keep them as hex for now
                // We'll convert to ANSI during rule compilation
                let mut invalid_colors = Vec::new();
                for (color_name, value) in config.palette.iter() {
                    if !is_valid_hex_color(value) {
                        log_warn!("Invalid hex color '{}' for palette entry '{}'", value, color_name);
                        invalid_colors.push(color_name.clone());
                    }
                }

                if !invalid_colors.is_empty() {
                    log_warn!("Found {} invalid color(s): {:?}", invalid_colors.len(), invalid_colors);
                }

                // Compile the rules
                let compiled_rules = compile_rules(&config);
                log_info!("Compiled {} highlight rules", compiled_rules.len());
                config.metadata.compiled_rule_set = compile_rule_set(&compiled_rules);
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

    // Reload pipeline for live config updates.
    /// Loads and applies new configuration.
    pub(crate) fn reload_config(self) -> Result<(), String> {
        log_info!("Reloading configuration...");
        let mut current_config = match super::get_config().write() {
            Ok(config_guard) => config_guard,
            Err(poisoned) => {
                log_error!("Configuration lock poisoned during reload; continuing with recovered state");
                poisoned.into_inner()
            }
        };

        let mut new_config = self.load_config().map_err(|err| {
            log_warn!("Failed to reload configuration: {}", err);
            err.to_string()
        })?;

        // Preserve session name across reloads
        new_config.metadata.session_name = current_config.metadata.session_name.clone();
        // Increment version to signal config change to active threads
        new_config.metadata.version = current_config.metadata.version.wrapping_add(1);

        *current_config = new_config;

        let rule_count = current_config.metadata.compiled_rules.len();
        let secret_count = current_config.metadata.compiled_secret_patterns.len();
        log_info!("Reloaded {} highlight rules", rule_count);
        if secret_count > 0 {
            log_info!("Reloaded {} secret redaction patterns", secret_count);
        }

        super::set_config_version(current_config.metadata.version);
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
    color[1..].chars().all(|hex_char| hex_char.is_ascii_hexdigit())
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding ANSI color code
fn compile_rules(config: &Config) -> Vec<CompiledHighlightRule> {
    let mut rules = Vec::new();
    let mut failed_rules = Vec::new();
    let mut missing_colors = Vec::new();

    for (idx, rule) in config.rules.iter().enumerate() {
        // Check if the referenced foreground color exists in the palette
        let fg_color = match config.palette.get(&rule.color) {
            Some(hex) => hex_to_ansi(hex, ColorType::Foreground),
            None => {
                missing_colors.push((idx + 1, rule.color.clone()));
                String::new()
            }
        };

        // Check if there's a background color specified
        let bg_color = if let Some(bg_name) = &rule.bg_color {
            match config.palette.get(bg_name) {
                Some(hex) => hex_to_ansi(hex, ColorType::Background),
                None => {
                    missing_colors.push((idx + 1, format!("{} (background)", bg_name)));
                    String::new()
                }
            }
        } else {
            String::new()
        };

        // Combine foreground and background codes
        let ansi_code = if !fg_color.is_empty() && !bg_color.is_empty() {
            // Both fg and bg: combine them into single escape sequence
            // Extract RGB values and combine: \x1b[38;2;r;g;b;48;2;r;g;bm
            let fg_params = &fg_color[2..fg_color.len() - 1]; // Remove \x1b[ and m
            let bg_params = &bg_color[2..bg_color.len() - 1];
            format!("\x1b[{};{}m", fg_params, bg_params)
        } else if !fg_color.is_empty() {
            fg_color
        } else if !bg_color.is_empty() {
            bg_color
        } else {
            "\x1b[0m".to_string() // Reset if no valid colors
        };

        // This is done to make sure newline characters are removed from the string before they are loaded into a Regex value
        // This will not remove the string value "\n" just actually new line characters Ex. "Hello\nWorld" will not have "\n" replaced because it is the string "\n" instead of the actual newline character
        let clean_regex = rule.regex.replace('\n', "").trim().to_string();

        match Regex::new(&clean_regex) {
            Ok(regex) => rules.push(CompiledHighlightRule::new(regex, ansi_code)),
            Err(err) => {
                log_warn!("Invalid regex in rule #{} ('{}'): {}", idx + 1, clean_regex, err);
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
        for (i, rule) in rules.iter().enumerate() {
            log_debug!(
                "Rule {}: regex = {:?}, color = {:?}, reset_mode = {:?}",
                i + 1,
                rule.regex,
                rule.style,
                rule.reset_mode
            );
        }
    }

    rules
}

fn compile_rule_set(rules: &[CompiledHighlightRule]) -> Option<RegexSet> {
    if rules.is_empty() {
        return None;
    }

    let patterns: Vec<&str> = rules.iter().map(|rule| rule.regex.as_str()).collect();
    match RegexSet::new(patterns) {
        Ok(regex_set) => Some(regex_set),
        Err(err) => {
            log_warn!("Failed to compile regex prefilter set: {}", err);
            None
        }
    }
}

/// Type of color to convert (foreground or background)
#[derive(Debug, Clone, Copy)]
enum ColorType {
    Foreground,
    Background,
}

/// Converts a hex color code (e.g., "#FFFFFF") to an ANSI escape sequence for terminal color
///
/// - `hex`: A string slice representing the hex color code
/// - `color_type`: Whether this is a foreground or background color
///
/// Returns a string containing the ANSI escape sequence for the RGB color, or an empty string if invalid
fn hex_to_ansi(hex: &str, color_type: ColorType) -> String {
    // Check if the hex code is valid (starts with '#' and has 7 characters)
    if hex.len() == 7 && hex.starts_with('#') {
        // Parse the red, green, and blue values from the hex string
        if let (Ok(red), Ok(green), Ok(blue)) = (
            u8::from_str_radix(&hex[1..3], 16),
            u8::from_str_radix(&hex[3..5], 16),
            u8::from_str_radix(&hex[5..7], 16),
        ) {
            // Return the ANSI escape sequence for the RGB color
            // 38 = foreground, 48 = background
            let code = match color_type {
                ColorType::Foreground => 38,
                ColorType::Background => 48,
            };
            return format!("\x1b[{};2;{};{};{}m", code, red, green, blue);
        }
    }
    // Return empty string if the hex is invalid (will use reset instead)
    String::new()
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
                }
            }
        }
    }

    patterns
}

#[cfg(test)]
#[path = "../test/config/loader.rs"]
mod tests;
