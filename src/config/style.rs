//! Configuration data structures and styling
//!
//! Defines the configuration schema for color-ssh, including:
//! - Application settings
//! - Color palette definitions
//! - Highlight rules with regex patterns
//! - Runtime metadata

use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::{collections::HashMap, path::PathBuf};

/// Main configuration structure loaded from YAML
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Application-wide settings
    #[serde(default)]
    pub settings: Settings,
    /// Interactive session-manager settings (optional block)
    #[serde(default, alias = "setting_interactive")]
    pub interactive_settings: Option<InteractiveSettings>,
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

/// Interactive-only session manager settings.
#[derive(Debug, Deserialize, Default)]
pub struct InteractiveSettings {
    /// History buffer size (scrollback lines for session manager tabs)
    #[serde(default = "default_history_buffer")]
    pub history_buffer: usize,
    /// Whether host tree folders should start collapsed in session manager
    #[serde(default = "default_host_tree_start_collapsed")]
    pub host_tree_start_collapsed: bool,
    /// Whether the host info pane is shown by default
    #[serde(default = "default_info_view")]
    pub info_view: bool,
    /// Host panel width as a percentage of terminal width
    #[serde(default = "default_host_view_size", deserialize_with = "deserialize_host_view_size")]
    pub host_view_size: u16,
    /// Host info pane height as a percentage of host panel height
    #[serde(default = "default_info_view_size", deserialize_with = "deserialize_info_view_size")]
    pub info_view_size: u16,
}

fn default_show_title() -> bool {
    true
}

fn default_history_buffer() -> usize {
    1000
}

fn default_host_tree_start_collapsed() -> bool {
    false
}

fn default_info_view() -> bool {
    true
}

fn default_host_view_size() -> u16 {
    25
}

fn default_info_view_size() -> u16 {
    40
}

fn deserialize_host_view_size<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u16::deserialize(deserializer)?;
    Ok(value.clamp(10, 70))
}

fn deserialize_info_view_size<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u16::deserialize(deserializer)?;
    Ok(value.clamp(10, 80))
}

/// A single highlight rule mapping a regex pattern to a color
#[derive(Debug, Deserialize)]
pub struct HighlightRule {
    /// Regex pattern to match (will be compiled at config load time)
    pub regex: String,
    /// Color name from the palette to apply to matches (foreground)
    pub color: String,
    /// Optional background color name from the palette
    #[serde(default)]
    pub bg_color: Option<String>,
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
    /// Pre-compiled secret redaction patterns
    #[serde(skip)]
    pub compiled_secret_patterns: Vec<Regex>,
    /// Version counter incremented on each config reload
    #[serde(skip)]
    pub version: u64,
}
