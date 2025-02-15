pub mod loader;
pub mod style;
pub mod watcher;

use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

// Load initial config, and compiled rules as statics so that they can be updated and changed when the socket calls a reload this also allows them to be used globally
pub static CONFIG: Lazy<Arc<RwLock<style::Config>>> = Lazy::new(|| {
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
