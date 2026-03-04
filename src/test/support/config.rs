use crate::config::{AuthSettings, Config, Metadata, Settings};
use std::collections::HashMap;

pub(crate) fn base_config() -> Config {
    Config {
        settings: Settings::default(),
        auth_settings: AuthSettings::default(),
        interactive_settings: None,
        palette: HashMap::new(),
        rules: Vec::new(),
        metadata: Metadata::default(),
    }
}
