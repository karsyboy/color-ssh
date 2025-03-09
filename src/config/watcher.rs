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
                    // println!("Event info {:?}\r", event);
                    tx.send(()).unwrap();
                }
            }
        },
        notify::Config::default(),
    )
    .expect("Failed to initialize file watcher\r");

    watcher
        .watch(
            Path::new(SESSION_CONFIG.read().unwrap().metadata.config_path.to_str().unwrap()),
            RecursiveMode::NonRecursive,
        )
        .expect("Failed to watch configuration file\r");

    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(()) => {
                    while let Ok(_) = rx.recv_timeout(Duration::from_millis(500)) {
                        // Keeps receiving events until itd done
                    }
                    println!("\r\n📝 Configuration change detected...\r");
                    let config_loader = ConfigLoader::new();
                    if let Err(err) = config_loader.reload_config() {
                        log_debug!("Error reloading config: {}", err);
                        eprintln!("❌Error reloading config: {}\r", err);
                    } else {
                        log_debug!("Configuration reloaded successfully");
                        println!("💾 Configuration reloaded [⌨️  Press Enter]:\r");
                    }
                }
                Err(err) => {
                    eprintln!("Error receiving from channel: {:?}\r", err);
                }
            }
        }
    });
    watcher // Return the watcher so it stays in scope
}
