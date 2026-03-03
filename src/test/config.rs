use super::*;
use crate::config::{AuthSettings, Config, Metadata, Settings};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

fn base_config() -> Config {
    Config {
        settings: Settings::default(),
        auth_settings: AuthSettings::default(),
        interactive_settings: None,
        palette: HashMap::new(),
        rules: Vec::new(),
        metadata: Metadata::default(),
    }
}

#[test]
fn replace_config_replaces_existing_auth_settings() {
    let shared = Arc::new(RwLock::new(base_config()));

    let mut updated = base_config();
    updated.auth_settings.unlock_idle_timeout_seconds = 42;
    updated.auth_settings.unlock_absolute_timeout_seconds = 84;

    replace_config(&shared, updated);

    let auth_settings = shared.read().expect("read config").auth_settings.clone();
    assert_eq!(auth_settings.unlock_idle_timeout_seconds, 42);
    assert_eq!(auth_settings.unlock_absolute_timeout_seconds, 84);
}
