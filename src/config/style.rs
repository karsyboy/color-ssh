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
    pub palette: HashMap<String, String>, // Map of color names (keys) to their respective hex codes (values)
    pub rules: Vec<HighlightRule>, // List of highlighting rules with a regex pattern and the corresponding color
    #[serde(default)]
    pub metadata: Metadata, // Metadata configuration
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
