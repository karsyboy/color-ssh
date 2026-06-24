//! Quick-connect profile discovery.

use crate::tui::AppState;
use crate::{args, config};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;

impl AppState {
    // Scan config directory for profile-specific config filenames.
    pub(crate) fn discover_quick_connect_profiles(&self) -> Vec<String> {
        let mut profiles: HashSet<String> = HashSet::new();
        profiles.insert("default".to_string());

        let config_dir = config::with_current_config("reading config directory for quick connect profiles", |cfg| {
            cfg.metadata.config_path.parent().map(|config_path| config_path.to_path_buf())
        });

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
                    && args::validate_profile_name(profile_name)
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
