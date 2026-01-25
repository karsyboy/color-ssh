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
use std::sync::{Arc, RwLock};

/// Global configuration instance
///
/// This is set once at startup (in main.rs) and can be updated via the config watcher.
/// Use `.read()` for read-only access and `.write()` for modifications.
///
/// # Examples
///
/// ```no_run
/// use csh::config::SESSION_CONFIG;
///
/// // Read configuration
/// let config = SESSION_CONFIG.get().unwrap();
/// let show_title = config.read().unwrap().settings.show_title;
/// // Or:
/// let show_title = SESSION_CONFIG.get().unwrap().read().unwrap().settings.show_title;
///
/// // Write configuration
/// config.write().unwrap().metadata.session_name = "example".to_string();
///  // Or:
/// SESSION_CONFIG.get().unwrap().write().unwrap().metadata.session_name = "example".to_string();
/// ```
pub static SESSION_CONFIG: OnceCell<Arc<RwLock<style::Config>>> = OnceCell::new();

/// Loads and initializes the global configuration with an optional profile.
/// Call this once in main.rs after parsing CLI args.
pub fn init_session_config(profile: Option<String>) -> Result<(), ConfigError> {
    let config_loader = loader::ConfigLoader::new(profile).map_err(|err| ConfigError::IoError(err))?;
    let config = config_loader.load_config().map_err(|err| ConfigError::IoError(err))?;
    SESSION_CONFIG.set(Arc::new(RwLock::new(config))).map_err(|_| ConfigError::AlreadyInitialized)?;
    Ok(())
}
