use super::{SESSION_CONFIG, loader::ConfigLoader};
use crate::log_debug;
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{path::Path, sync::mpsc, thread, time::Duration};

pub fn config_watcher() -> RecommendedWatcher {
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, Error>| {
            if let Ok(event) = res {
                if event.kind.is_modify() {
                    tx.send(()).unwrap();
                }
            }
        },
        notify::Config::default(),
    )
    .unwrap_or_else(|err| {
        panic!("❌ Failed to create watcher: {}", err);
    });

    watcher
        .watch(
            Path::new(SESSION_CONFIG.read().unwrap().metadata.config_path.to_str().unwrap()),
            RecursiveMode::NonRecursive,
        )
        .unwrap_or_else(|err| {
            eprintln!("❌ Failed to watch config file: {}", err);
        });

    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(()) => {
                    while let Ok(_) = rx.recv_timeout(Duration::from_millis(500)) {}
                    println!("\r\n📝 Configuration change detected...\r");
                    let config_loader = ConfigLoader::new();
                    if let Err(err) = config_loader.reload_config() {
                        log_debug!("Error reloading config: {}", err);
                        eprintln!("❌ Error reloading config: {}", err);
                    } else {
                        log_debug!("Configuration reloaded successfully");
                        println!("💾 Configuration reloaded [⌨️  Press Enter]:\r");
                    }
                }
                Err(err) => {
                    eprintln!("❌ Error receiving from channel: {}", err);
                }
            }
        }
    });
    watcher
}
