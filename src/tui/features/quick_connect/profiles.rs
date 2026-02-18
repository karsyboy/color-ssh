//! Quick-connect profile discovery.

use crate::config;
use crate::tui::SessionManager;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;

impl SessionManager {
    pub(crate) fn discover_quick_connect_profiles(&self) -> Vec<String> {
        let mut profiles: HashSet<String> = HashSet::new();
        profiles.insert("default".to_string());

        let config_dir = config::SESSION_CONFIG
            .get()
            .and_then(|config_lock| {
                config_lock
                    .read()
                    .ok()
                    .map(|cfg| cfg.metadata.config_path.parent().map(|config_path| config_path.to_path_buf()))
            })
            .flatten();

        if let Some(config_dir) = config_dir
            && let Ok(entries) = fs::read_dir(config_dir)
        {
            for entry in entries.flatten() {
                let filename = entry.file_name();
                let Some(filename) = filename.to_str() else {
                    continue;
                };

                if filename == "cossh-config.yaml" {
                    profiles.insert("default".to_string());
                    continue;
                }

                if let Some(profile_name) = filename.strip_suffix(".cossh-config.yaml")
                    && !profile_name.is_empty()
                    && !profile_name.starts_with('.')
                {
                    profiles.insert(profile_name.to_string());
                }
            }
        }

        let mut profile_list: Vec<String> = profiles.into_iter().collect();
        profile_list.sort_by(
            |left, right| match (left.eq_ignore_ascii_case("default"), right.eq_ignore_ascii_case("default")) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => left.to_lowercase().cmp(&right.to_lowercase()),
            },
        );

        profile_list
    }
}
