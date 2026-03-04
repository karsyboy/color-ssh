//! Config file watch and hot-reload loop.

use super::loader::ConfigLoader;
use crate::{log_debug, log_error, log_info, log_warn};
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

fn event_targets_config_file(event: &Event, config_path: &Path) -> bool {
    let config_file_name = config_path.file_name();
    event.paths.iter().any(|path| {
        path == config_path
            || match (path.file_name(), config_file_name) {
                (Some(event_name), Some(config_name)) => event_name == config_name,
                _ => false,
            }
    })
}

fn should_reload_for_event(event: &Event, config_path: &Path) -> bool {
    (event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove()) && event_targets_config_file(event, config_path)
}

fn print_reload_notice(message: &str) {
    // Render notices on a clean line so they do not collide with remote shell prompts.
    eprint!("\r\n[color-ssh] {}\r\n", message);
    let _ = io::stderr().flush();
}

/// Start watching the active config file for changes.
pub fn config_watcher(profile: Option<String>) -> Option<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel();

    log_debug!("Initializing configuration file watcher");

    let config_path = super::with_current_config("starting watcher", |cfg| cfg.metadata.config_path.clone());
    let config_path_clone = config_path.clone();

    let mut watcher = match RecommendedWatcher::new(
        move |res: Result<Event, Error>| {
            if let Ok(event) = res
                && should_reload_for_event(&event, &config_path_clone)
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

    // Watch the parent directory to catch atomic rename writes.
    let fallback = PathBuf::from(".");
    let watch_path = config_path.parent().unwrap_or(&fallback);
    log_info!("Starting config watcher for: {:?} (watching directory: {:?})", config_path, watch_path);

    if let Err(err) = watcher.watch(watch_path, RecursiveMode::NonRecursive) {
        log_error!("Failed to watch config directory: {}", err);
        log_warn!("Configuration hot-reload disabled");
        return None;
    }

    if let Err(err) = thread::Builder::new().name("config-watcher".to_string()).spawn(move || {
        log_debug!("Config watcher thread started");
        loop {
            match rx.recv() {
                Ok(()) => {
                    // Debounce bursty editor writes before one reload attempt.
                    while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}

                    log_info!("Configuration change detected, reloading...");

                    let config_loader = match ConfigLoader::new(profile.clone()) {
                        Ok(loader) => loader,
                        Err(err) => {
                            log_error!("Error creating config loader for reload: {}", err);
                            print_reload_notice(&format!("Config reload failed: {}", err));
                            continue;
                        }
                    };
                    if let Err(err) = config_loader.reload_config() {
                        log_error!("Error reloading config: {}", err);
                        print_reload_notice(&format!("Config reload failed: {}", err));
                    } else {
                        log_info!("Configuration reloaded successfully");
                        print_reload_notice("Config reloaded successfully");
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
#[path = "../test/config/watcher.rs"]
mod tests;
