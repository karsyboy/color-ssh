use dirs::home_dir;
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    pub palette: HashMap<String, String>,
    pub rules: Vec<HighlightRule>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub vault_path: Option<PathBuf>,
    pub vault_key: Option<PathBuf>,
    #[serde(default)]
    pub remove_secrets: Option<Vec<String>>,
    #[serde(default = "default_show_title")]
    pub show_title: bool,
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default)]
    pub ssh_logging: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vault_path: home_dir().map(|mut path| {
                path.push(".csh");
                path.push("vault");
                path.push("vault.kdbx");
                path
            }),
            vault_key: None,
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

#[derive(Debug, Deserialize)]
pub struct HighlightRule {
    pub regex: String,
    pub color: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub config_path: PathBuf,
    pub session_name: String,
    #[serde(skip)]
    pub compiled_rules: Vec<(Regex, String)>,
}
