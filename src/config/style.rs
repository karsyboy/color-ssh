//! Configuration data structures and styling
//!
//! Defines the configuration schema for color-ssh, including:
//! - Application settings
//! - Color palette definitions
//! - Highlight rules with regex patterns
//! - Runtime metadata

use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

/// Main configuration structure loaded from YAML
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Application-wide settings
    #[serde(default)]
    pub settings: Settings,
    /// Color palette mapping names to hex codes (converted to ANSI at runtime)
    pub palette: HashMap<String, String>,
    /// Syntax highlighting rules
    pub rules: Vec<HighlightRule>,
    /// Runtime metadata (not from config file)
    #[serde(default)]
    pub metadata: Metadata,
}

/// Application settings
#[derive(Debug, Deserialize)]
pub struct Settings {

    /// Regex patterns for secrets to redact from logs
    #[serde(default)]
    pub remove_secrets: Option<Vec<String>>,
    /// Whether to show the ASCII art banner on startup
    #[serde(default = "default_show_title")]
    pub show_title: bool,
    /// Enable debug logging
    #[serde(default)]
    pub debug_mode: bool,
    /// Enable SSH session logging
    #[serde(default)]
    pub ssh_logging: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remove_secrets: None,
            show_title: true,
            debug_mode: false,
            ssh_logging: false,
        }
    }
}

fn default_show_title() -> bool {
    true
}

/// A single highlight rule mapping a regex pattern to a color
#[derive(Debug, Deserialize)]
pub struct HighlightRule {
    /// Regex pattern to match (will be compiled at config load time)
    pub regex: String,
    /// Color name from the palette to apply to matches
    pub color: String,
}

/// Runtime metadata not stored in config file
#[derive(Debug, Deserialize, Default)]
pub struct Metadata {
    /// Path to the loaded configuration file
    #[serde(default)]
    pub config_path: PathBuf,
    /// Name of the current SSH session (for log file naming)
    pub session_name: String,
    /// Compiled regex rules (regex + ANSI color code)
    #[serde(skip)]
    pub compiled_rules: Vec<(Regex, String)>,
}
