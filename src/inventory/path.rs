//! Path helpers for inventory discovery and expansion.

use std::path::PathBuf;

/// Expand `~/` to the current user's home directory.
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen('~', &home.to_string_lossy(), 1);
    }

    path.to_string()
}

/// Returns `~/.color-ssh/cossh-inventory.yaml`.
pub fn get_default_inventory_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".color-ssh").join("cossh-inventory.yaml"))
}
