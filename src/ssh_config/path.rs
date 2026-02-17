//! Path helpers for SSH config discovery and expansion.

use std::path::PathBuf;

/// Expand `~/` to the current user's home directory.
pub(super) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen('~', &home.to_string_lossy(), 1);
    }

    path.to_string()
}

/// Get the default SSH config path.
pub fn get_default_ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}
