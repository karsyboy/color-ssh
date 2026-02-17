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
            Some(profile_name) if !profile_name.is_empty() => format!("{}.cossh-config.yaml", profile_name),
            _ => ".cossh-config.yaml".to_string(),
        };

        // Check first possible location: ~/.color-ssh/{profile}.cossh-config.yaml
        if let Some(home_dir) = dirs::home_dir() {
            let cossh_dir_path = home_dir.join(".color-ssh").join(&config_filename);
            log_debug!("Checking: {:?}", cossh_dir_path);
            if cossh_dir_path.exists() {
                log_info!("Found config at: {:?}", cossh_dir_path);
                return Ok(cossh_dir_path);
            }
        }

        // Check second possible location: ~/{profile}.cossh-config.yaml
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
        let cossh_dir = home_dir.join(".color-ssh");
        let config_path = cossh_dir.join(".cossh-config.yaml");

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
    color[1..].chars().all(|hex_char| hex_char.is_ascii_hexdigit())
}

/// Compiles the highlighting rules from the configuration into a vector of regex patterns and their corresponding colors
///
///  - `config`: A reference to the Config struct containing the color palette and highlighting rules
///
/// Returns a vector of tuples, each containing a regex pattern and the corresponding ANSI color code
fn compile_rules(config: &Config) -> Vec<(Regex, String)> {
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
            Ok(regex) => rules.push((regex, ansi_code)),
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
                    eprintln!("Warning: Invalid secret pattern '{}' - {}\r", pattern, err);
                }
            }
        }
    }

    patterns
}

#[cfg(test)]
mod tests {
    use super::{ColorType, compile_rules, compile_secret_patterns, hex_to_ansi, is_valid_hex_color};
    use crate::config::style::{Config, HighlightRule, Metadata, Settings};
    use std::collections::HashMap;

    fn base_config() -> Config {
        Config {
            settings: Settings::default(),
            interactive_settings: None,
            palette: HashMap::new(),
            rules: Vec::new(),
            metadata: Metadata::default(),
        }
    }

    #[test]
    fn validates_hex_color_format() {
        assert!(is_valid_hex_color("#00ffAA"));
        assert!(!is_valid_hex_color("00ffAA"));
        assert!(!is_valid_hex_color("#00ffA"));
        assert!(!is_valid_hex_color("#00ffZZ"));
    }

    #[test]
    fn converts_hex_to_ansi_for_fg_and_bg() {
        assert_eq!(hex_to_ansi("#112233", ColorType::Foreground), "\x1b[38;2;17;34;51m");
        assert_eq!(hex_to_ansi("#112233", ColorType::Background), "\x1b[48;2;17;34;51m");
        assert_eq!(hex_to_ansi("oops", ColorType::Foreground), "");
    }

    #[test]
    fn compiles_rules_and_handles_missing_colors_and_invalid_regex() {
        let mut config = base_config();
        config.palette.insert("ok_fg".to_string(), "#00ff00".to_string());
        config.palette.insert("ok_bg".to_string(), "#0000ff".to_string());
        config.rules = vec![
            HighlightRule {
                regex: "success".to_string(),
                color: "ok_fg".to_string(),
                bg_color: None,
            },
            HighlightRule {
                regex: "combo".to_string(),
                color: "ok_fg".to_string(),
                bg_color: Some("ok_bg".to_string()),
            },
            HighlightRule {
                regex: "fallback".to_string(),
                color: "missing".to_string(),
                bg_color: None,
            },
            HighlightRule {
                regex: "[unclosed".to_string(),
                color: "ok_fg".to_string(),
                bg_color: None,
            },
        ];

        let compiled = compile_rules(&config);
        assert_eq!(compiled.len(), 3, "invalid regex should be dropped");
        assert_eq!(compiled[0].1, "\x1b[38;2;0;255;0m");
        assert_eq!(compiled[1].1, "\x1b[38;2;0;255;0;48;2;0;0;255m");
        assert_eq!(compiled[2].1, "\x1b[0m", "missing palette entry should fall back to reset");
    }

    #[test]
    fn compiles_only_valid_secret_patterns() {
        let mut config = base_config();
        config.settings.remove_secrets = Some(vec!["token=\\w+".to_string(), "[".to_string()]);

        let patterns = compile_secret_patterns(&config);
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].is_match("token=abc123"));
    }
}
