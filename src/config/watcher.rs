//! Configuration file watching and hot-reloading
//!
//! Monitors the configuration file for changes and automatically reloads
//! when modifications are detected.

use super::loader::ConfigLoader;
use crate::{log_debug, log_error, log_info, log_warn};
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{path::PathBuf, sync::mpsc, thread, time::Duration};

fn event_targets_config_file(event: &Event, config_file_name: &str) -> bool {
    event.paths.iter().any(|path| {
        path.file_name()
            .and_then(|segment| segment.to_str())
            .map(|name| name == config_file_name)
            .unwrap_or(false)
    })
}

fn should_reload_for_event(event: &Event, config_file_name: &str) -> bool {
    (event.kind.is_modify() || event.kind.is_create()) && event_targets_config_file(event, config_file_name)
}

/// Start watching the configuration file for changes
pub fn config_watcher(profile: Option<String>) -> Option<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel();

    log_debug!("Initializing configuration file watcher");

    let config_path = super::get_config().read().unwrap().metadata.config_path.clone();
    let config_file_name = config_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("").to_string();

    // Clone for use in the closure
    let config_file_name_clone = config_file_name.clone();

    let mut watcher = match RecommendedWatcher::new(
        move |res: Result<Event, Error>| {
            if let Ok(event) = res
                && should_reload_for_event(&event, &config_file_name_clone)
            {
                log_debug!("Config file change detected: {:?}", event);
                let _ = tx.send(());
            }
        },
        notify::Config::default(),
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            log_error!("Failed to create file watcher: {}", err);
            log_warn!("Configuration hot-reload disabled");
            return None;
        }
    };

    // Watch the parent directory to handle atomic writes (temp file + rename)
    let fallback = PathBuf::from(".");
    let watch_path = config_path.parent().unwrap_or(&fallback);
    log_info!("Starting config watcher for: {:?} (watching directory: {:?})", config_path, watch_path);

    if let Err(err) = watcher.watch(watch_path, RecursiveMode::NonRecursive) {
        log_error!("Failed to watch config directory: {}", err);
        eprintln!("Failed to watch config directory: {}", err);
        log_warn!("Configuration hot-reload disabled");
        return None;
    }

    // Spawn a named thread for config watching
    if let Err(err) = thread::Builder::new().name("config-watcher".to_string()).spawn(move || {
        log_debug!("Config watcher thread started");
        loop {
            match rx.recv() {
                Ok(()) => {
                    // Debounce: wait for additional events and discard them
                    while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}

                    log_info!("Configuration change detected, reloading...");
                    eprintln!("\r\nConfiguration change detected...\r");

                    let config_loader = match ConfigLoader::new(profile.clone()) {
                        Ok(loader) => loader,
                        Err(err) => {
                            log_error!("Error creating config loader for reload: {}", err);
                            eprintln!("Error creating config loader for reload: {}", err);
                            continue;
                        }
                    };
                    if let Err(err) = config_loader.reload_config() {
                        log_error!("Error reloading config: {}", err);
                        eprintln!("Error reloading config: {}\r", err);
                    } else {
                        log_info!("Configuration reloaded successfully");
                        eprintln!("Configuration reloaded [Press Enter]:\r");
                    }
                }
                Err(err) => {
                    log_error!("Error receiving from channel: {}", err);
                    break;
                }
            }
        }
    }) {
        log_error!("Failed to spawn config watcher thread: {}", err);
        log_warn!("Configuration hot-reload disabled");
        return None;
    }

    Some(watcher)
}

#[cfg(test)]
mod tests {
    use super::should_reload_for_event;
    use notify::{
        Event,
        event::{CreateKind, EventKind, ModifyKind, RemoveKind},
    };
    use std::path::PathBuf;

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        Event {
            kind,
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: Default::default(),
        }
    }

    #[test]
    fn reloads_only_for_modify_or_create_on_target_file() {
        let config_name = "cossh-config.yaml";
        let modify_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/cossh-config.yaml"]);
        let create_event = event(EventKind::Create(CreateKind::Any), &["/tmp/cossh-config.yaml"]);
        let wrong_file = event(EventKind::Modify(ModifyKind::Any), &["/tmp/other.yaml"]);
        let remove_event = event(EventKind::Remove(RemoveKind::Any), &["/tmp/cossh-config.yaml"]);

        assert!(should_reload_for_event(&modify_event, config_name));
        assert!(should_reload_for_event(&create_event, config_name));
        assert!(!should_reload_for_event(&wrong_file, config_name));
        assert!(!should_reload_for_event(&remove_event, config_name));
    }
}
