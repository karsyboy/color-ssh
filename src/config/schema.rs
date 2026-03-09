//! Config schema definitions deserialized from YAML.
//!
//! This module defines stable user-facing config fields and runtime metadata
//! attached after parsing.

use crate::highlighter::CompiledHighlightRule;
use regex::{Regex, RegexSet};
use serde::{Deserialize, Deserializer};
use std::{collections::HashMap, path::PathBuf};

/// Main configuration structure loaded from YAML
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Application-wide settings
    #[serde(default)]
    pub settings: Settings,
    /// Authentication and vault settings.
    #[serde(default)]
    pub auth_settings: AuthSettings,
    /// Interactive session-manager settings (optional block)
    #[serde(default)]
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
#[serde(deny_unknown_fields)]
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

/// Authentication settings for the shared password vault.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthSettings {
    /// Idle timeout in seconds before the unlock agent relocks the vault.
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: u64,
    /// Maximum unlock lifetime in seconds before the agent relocks regardless of activity.
    #[serde(default = "default_session_timeout_seconds")]
    pub session_timeout_seconds: u64,
    /// Whether direct `cossh ssh host` launches should attempt password auto-login.
    #[serde(default = "default_direct_password_autologin")]
    pub direct_password_autologin: bool,
    /// Whether TUI launches should attempt password auto-login.
    #[serde(default = "default_tui_password_autologin")]
    pub tui_password_autologin: bool,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            idle_timeout_seconds: default_idle_timeout_seconds(),
            session_timeout_seconds: default_session_timeout_seconds(),
            direct_password_autologin: default_direct_password_autologin(),
            tui_password_autologin: default_tui_password_autologin(),
        }
    }
}

/// Interactive-only session manager settings.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct InteractiveSettings {
    /// History buffer size (scrollback lines for session manager tabs)
    #[serde(default = "default_history_buffer")]
    pub history_buffer: usize,
    /// Whether host tree folders should start uncollapsed in session manager.
    /// `false` (default) means the tree starts collapsed.
    #[serde(default = "default_host_tree_uncollapsed")]
    pub host_tree_uncollapsed: bool,
    /// Whether the host info pane is shown by default
    #[serde(default = "default_info_view")]
    pub info_view: bool,
    /// Host panel width as a percentage of terminal width
    #[serde(default = "default_host_view_size", deserialize_with = "deserialize_host_view_size")]
    pub host_view_size: u16,
    /// Host info pane height as a percentage of host panel height
    #[serde(default = "default_info_view_size", deserialize_with = "deserialize_info_view_size")]
    pub info_view_size: u16,
    /// Allow remote OSC 52 clipboard write requests emitted by SSH sessions.
    /// Disabled by default for safety.
    #[serde(default = "default_remote_clipboard_write")]
    pub allow_remote_clipboard_write: bool,
    /// Maximum clipboard payload size accepted from remote OSC 52 requests.
    #[serde(default = "default_remote_clipboard_max_bytes", deserialize_with = "deserialize_remote_clipboard_max_bytes")]
    pub remote_clipboard_max_bytes: usize,
    /// Highlight overlay behavior for embedded terminal rendering.
    #[serde(default)]
    pub overlay_highlighting: HighlightOverlayMode,
}

/// Renderer-side syntax highlighting policy for embedded terminal views.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightOverlayMode {
    /// Enable highlighting when the renderer considers the viewport safe.
    #[default]
    Auto,
    /// Always attempt overlay highlighting, even for alternate-screen apps.
    Always,
    /// Disable renderer-side overlay highlighting entirely.
    Off,
}

fn default_show_title() -> bool {
    true
}

fn default_history_buffer() -> usize {
    1000
}

fn default_idle_timeout_seconds() -> u64 {
    900
}

fn default_session_timeout_seconds() -> u64 {
    28_800
}

fn default_direct_password_autologin() -> bool {
    true
}

fn default_tui_password_autologin() -> bool {
    true
}

fn default_host_tree_uncollapsed() -> bool {
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

fn default_remote_clipboard_write() -> bool {
    false
}

fn default_remote_clipboard_max_bytes() -> usize {
    4096
}

fn deserialize_host_view_size<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    // Clamp persisted values so invalid config does not break layout math.
    let value = u16::deserialize(deserializer)?;
    Ok(value.clamp(10, 70))
}

fn deserialize_info_view_size<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    // Clamp persisted values so invalid config does not break layout math.
    let value = u16::deserialize(deserializer)?;
    Ok(value.clamp(10, 80))
}

fn deserialize_remote_clipboard_max_bytes<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    // Keep clipboard payload bounds within a safe and practical range.
    let value = usize::deserialize(deserializer)?;
    Ok(value.clamp(64, 1_048_576))
}

/// A single highlight rule mapping a regex pattern to a color
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighlightRule {
    /// Regex pattern to match (will be compiled at config load time)
    pub regex: String,
    /// Color name from the palette to apply to matches (foreground)
    pub color: String,
    /// Optional user-facing description for this rule (not used by runtime matching)
    #[serde(default)]
    pub description: Option<String>,
    /// Optional background color name from the palette
    #[serde(default)]
    pub bg_color: Option<String>,
}

/// Runtime metadata not stored in config file
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    /// Path to the loaded configuration file
    #[serde(default)]
    pub config_path: PathBuf,
    /// Name of the current SSH session (for log file naming)
    pub session_name: String,
    /// Compiled regex rules (regex + ANSI color code)
    #[serde(skip)]
    pub(crate) compiled_rules: Vec<CompiledHighlightRule>,
    /// Regex-set prefilter used to cheaply identify rules that might match a chunk.
    #[serde(skip)]
    pub compiled_rule_set: Option<RegexSet>,
    /// Pre-compiled secret redaction patterns
    #[serde(skip)]
    pub compiled_secret_patterns: Vec<Regex>,
    /// Version counter incremented on each config reload
    #[serde(skip)]
    pub version: u64,
}
