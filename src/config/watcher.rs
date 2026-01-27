//! Configuration file watching and hot-reloading
//!
//! Monitors the configuration file for changes and automatically reloads
//! when modifications are detected.

use super::loader::ConfigLoader;
use crate::{log_debug, log_error, log_info};
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{sync::mpsc, thread, time::Duration};

/// Start watching the configuration file for changes
pub fn config_watcher(profile: Option<String>) -> RecommendedWatcher {
    let (tx, rx) = mpsc::channel();

    log_debug!("Initializing configuration file watcher");

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, Error>| {
            if let Ok(event) = res
                && event.kind.is_modify(){
                    log_debug!("Config file modification detected: {:?}", event);
                    let _ = tx.send(());
                }
            },
        notify::Config::default(),
    )
    .unwrap_or_else(|err| {
        log_error!("Failed to create file watcher: {}", err);
        panic!("Failed to create watcher: {}", err);
    });

    let config_path = super::get_config().read().unwrap().metadata.config_path.clone();
    log_info!("Starting config watcher for: {:?}", config_path);

    watcher.watch(&config_path, RecursiveMode::NonRecursive).unwrap_or_else(|err| {
        log_error!("Failed to watch config file: {}", err);
        eprintln!("Failed to watch config file: {}", err);
    });

    // Spawn a named thread for config watching
    thread::Builder::new()
        .name("config-watcher".to_string())
        .spawn(move || {
            log_debug!("Config watcher thread started");
            loop {
                match rx.recv() {
                    Ok(()) => {
                        // Debounce: wait for additional events and discard them
                        while rx.recv_timeout(Duration::from_millis(500)).is_ok() {}

                        log_info!("Configuration change detected, reloading...");
                        println!("\r\nConfiguration change detected...\r");

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
                            println!("Configuration reloaded [Press Enter]:\r");
                        }
                    }
                    Err(err) => {
                        log_error!("Error receiving from channel: {}", err);
                    }
                }
            }
        })
        .unwrap_or_else(|err| {
            log_error!("Failed to spawn config watcher thread: {}", err);
            panic!("Failed to spawn config watcher thread: {}", err);
        });

    watcher
}
