use super::*;
use crate::test::support::config::base_config;
use std::sync::{Arc, RwLock};

#[test]
fn replace_config_existing_auth_settings_are_replaced() {
    let shared = Arc::new(RwLock::new(base_config()));

    let mut updated = base_config();
    updated.auth_settings.unlock_idle_timeout_seconds = 42;
    updated.auth_settings.unlock_absolute_timeout_seconds = 84;

    replace_config(&shared, updated);

    let auth_settings = shared.read().expect("read config").auth_settings.clone();
    assert_eq!(auth_settings.unlock_idle_timeout_seconds, 42);
    assert_eq!(auth_settings.unlock_absolute_timeout_seconds, 84);
}
