use notify::{event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::f32::consts::E;
use std::sync::mpsc::channel;
use std::path::Path;
use std::thread;
use crate::config::reload_config;

pub fn watch_config( config_path: &str) -> notify::Result<()> {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx, notify::Config::default())?;
    watcher.watch(Path::new(config_path), RecursiveMode::NonRecursive)?;

    thread::spawn(move ||{
        for res in rx {
            match res {
                Ok(event) => println!("event: {:?}", event),
                Err(e) => println!("watch error: {:?}", e)
            }
        }
    });
    Ok(())
}