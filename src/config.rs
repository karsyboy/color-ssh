//! Configuration management module
//!
//! Provides:
//! - Configuration file loading from multiple locations
//! - YAML parsing and validation
//! - Hot-reloading via file watching
//! - Global configuration access via thread-safe static
//!
//! The configuration is loaded once at startup and stored in a global
//! [`SESSION_CONFIG`] static that can be accessed from anywhere in the application.

mod errors;
mod loader;
mod style;
mod watcher;

pub use errors::ConfigError;
pub use watcher::config_watcher;

use once_cell::sync::OnceCell;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

/// Global configuration instance
///
/// This is set once at startup (in main.rs) and can be updated via the config watcher.
/// Use `.read()` for read-only access and `.write()` for modifications.
///
/// # Examples
///
/// ```no_run
/// use cossh::config;
///
/// // Read configuration using helper
/// let config = config::get_config();
/// let show_title = config.read().map(|guard| guard.settings.show_title).unwrap_or(true);
///
/// // Write configuration using helper
/// if let Ok(mut guard) = config::get_config().write() {
///     guard.metadata.session_name = "example".to_string();
/// }
/// ```
pub static SESSION_CONFIG: OnceCell<Arc<RwLock<style::Config>>> = OnceCell::new();
static CONFIG_VERSION: AtomicU64 = AtomicU64::new(0);

fn fallback_config() -> style::Config {
    style::Config {
        settings: style::Settings::default(),
        interactive_settings: None,
        palette: std::collections::HashMap::new(),
        rules: Vec::new(),
        metadata: style::Metadata {
            session_name: "session".to_string(),
            ..Default::default()
        },
    }
}

/// Get a reference to the global configuration
pub fn get_config() -> &'static Arc<RwLock<style::Config>> {
    SESSION_CONFIG.get_or_init(|| Arc::new(RwLock::new(fallback_config())))
}

/// Loads and initializes the global configuration with an optional profile.
/// Call this once in main.rs after parsing CLI args.
pub fn init_session_config(profile: Option<String>) -> Result<(), ConfigError> {
    let config_loader = loader::ConfigLoader::new(profile).map_err(ConfigError::IoError)?;
    let config = config_loader.load_config().map_err(ConfigError::IoError)?;
    set_config_version(config.metadata.version);
    SESSION_CONFIG.set(Arc::new(RwLock::new(config))).map_err(|_| ConfigError::AlreadyInitialized)?;
    Ok(())
}

/// Load `interactive_settings.history_buffer` for a specific profile, if available.
pub(crate) fn history_buffer_for_profile(profile: Option<&str>) -> Option<usize> {
    let profile = profile?.trim();
    if profile.is_empty() {
        return None;
    }

    let config_loader = loader::ConfigLoader::new(Some(profile.to_string())).ok()?;
    let config = config_loader.load_config().ok()?;
    config.interactive_settings.map(|interactive| interactive.history_buffer)
}

pub(crate) fn current_config_version() -> u64 {
    CONFIG_VERSION.load(Ordering::Acquire)
}

pub(crate) fn set_config_version(version: u64) {
    CONFIG_VERSION.store(version, Ordering::Release);
}
