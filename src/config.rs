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

use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

/// Global configuration instance
///
/// This is loaded once at startup and can be updated via the config watcher.
/// Use `.read()` for read-only access and `.write()` for modifications.
///
/// # Examples
///
/// ```no_run
/// use csh::config::SESSION_CONFIG;
///
/// // Read configuration
/// let show_title = SESSION_CONFIG.read().unwrap().settings.show_title;
///
/// // Write configuration
/// SESSION_CONFIG.write().unwrap().metadata.session_name = "example".to_string();
/// ```
pub static SESSION_CONFIG: Lazy<Arc<RwLock<style::Config>>> = Lazy::new(|| {
    Arc::new(RwLock::new({
        let config_loader = loader::ConfigLoader::new();
        let config = {
            config_loader.load_config().unwrap_or_else(|err| {
                eprintln!("Failed to load configuration: {}", err);
                std::process::exit(1);
            })
        };
        config
    }))
});
