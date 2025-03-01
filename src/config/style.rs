/*
TODO:
    - Add a settings struct to Config that contains user specified settings for this like:
        - Vault path
        - Attempt to remove passwords from ssh log file
        - debug mode
        - ssh logging
*/

use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub settings: Settings,               // User settings for the application
    pub palette: HashMap<String, String>, // Map of color names (keys) to their respective hex codes (values)
    pub rules: Vec<HighlightRule>, // List of highlighting rules with a regex pattern and the corresponding color
    #[serde(default)]
    pub metadata: Metadata, // Metadata configuration
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub vault_path: Option<PathBuf>,         // Path to the vault
    pub vault_key: Option<PathBuf>,          // Path to the vault
    #[serde(default)]
    pub remove_passwords_from_ssh_log: bool, // Flag to indicate if passwords should be removed from SSH logs
    #[serde(default)]
    pub debug_mode: bool,                    // Flag to enable debug mode
    #[serde(default)]
    pub ssh_logging: bool,                   // Flag to enable SSH logging
}

//create defaults for settings
impl Default for Settings {
    fn default() -> Self {
        Self {
            vault_path: Some(PathBuf::new()), // Default vault path
            vault_key: Some(PathBuf::new()),  // Default vault key path
            remove_passwords_from_ssh_log: false,
            debug_mode: false,
            ssh_logging: false,
        }
    }
}

// Structure representing a single highlight rule
#[derive(Debug, Deserialize)]
pub struct HighlightRule {
    pub regex: String, // Regex pattern to match text for highlighting
    pub color: String, // Color name (key in the palette) to use for the matched text
}

#[derive(Debug, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub config_path: PathBuf, // Path to the configuration file
    pub session_name: String, // Name of the current session
    #[serde(skip)]
    pub compiled_rules: Vec<(Regex, String)>, // Compiled regex rules for highlighting
}
