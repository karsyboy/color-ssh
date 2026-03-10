//! Config file watch and hot-reload loop.

use super::loader::ConfigLoader;
use crate::{log_debug, log_error, log_info, log_warn};
use notify::{Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashSet, VecDeque},
    env,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};

const MAX_PENDING_RELOAD_NOTICES: usize = 16;
const PROFILE_CONFIG_SUFFIX: &str = ".cossh-config.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadNoticeTarget {
    Stderr,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWatchScope {
    ActiveProfileOnly,
    AllProfiles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileReloadEvent {
    pub(crate) profile: String,
    pub(crate) message: String,
    pub(crate) success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingReloadEvent {
    ActiveConfig,
    Profile(String),
}

static RELOAD_NOTICE_QUEUE: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static PROFILE_RELOAD_QUEUE: OnceLock<Mutex<VecDeque<ProfileReloadEvent>>> = OnceLock::new();

fn with_reload_notice_queue<T>(f: impl FnOnce(&mut VecDeque<String>) -> T) -> T {
    match RELOAD_NOTICE_QUEUE.get_or_init(|| Mutex::new(VecDeque::new())).lock() {
        Ok(mut queue) => f(&mut queue),
        Err(poisoned) => {
            let mut queue = poisoned.into_inner();
            f(&mut queue)
        }
    }
}

fn with_profile_reload_queue<T>(f: impl FnOnce(&mut VecDeque<ProfileReloadEvent>) -> T) -> T {
    match PROFILE_RELOAD_QUEUE.get_or_init(|| Mutex::new(VecDeque::new())).lock() {
        Ok(mut queue) => f(&mut queue),
        Err(poisoned) => {
            let mut queue = poisoned.into_inner();
            f(&mut queue)
        }
    }
}

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

pub(crate) fn queue_reload_notice(message: impl Into<String>) {
    let message = message.into();
    with_reload_notice_queue(|queue| {
        if queue.len() >= MAX_PENDING_RELOAD_NOTICES {
            queue.pop_front();
        }
        queue.push_back(message);
    });
}

#[cfg(test)]
pub(crate) fn queue_profile_reload_event(event: ProfileReloadEvent) {
    with_profile_reload_queue(|queue| {
        if queue.len() >= MAX_PENDING_RELOAD_NOTICES {
            queue.pop_front();
        }
        queue.push_back(event);
    });
}

pub(crate) fn take_reload_notices() -> Vec<String> {
    with_reload_notice_queue(|queue| queue.drain(..).collect())
}

pub(crate) fn take_profile_reload_events() -> Vec<ProfileReloadEvent> {
    with_profile_reload_queue(|queue| queue.drain(..).collect())
}

fn emit_reload_notice(message: &str, target: ReloadNoticeTarget) {
    match target {
        ReloadNoticeTarget::Stderr => {
            // Render notices on a clean line so they do not collide with remote shell prompts.
            eprint!("\r\n[color-ssh] {}\r\n", message);
            let _ = io::stderr().flush();
        }
        ReloadNoticeTarget::Queue => queue_reload_notice(message.to_string()),
    }
}

fn queue_profile_reload_result(profile: String, success: bool, message: String) {
    with_profile_reload_queue(|queue| {
        if queue.len() >= MAX_PENDING_RELOAD_NOTICES {
            queue.pop_front();
        }
        queue.push_back(ProfileReloadEvent { profile, message, success });
    });
}

fn profile_name_from_config_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if file_name == super::paths::DEFAULT_CONFIG_FILENAME {
        return None;
    }

    let profile_name = file_name.strip_suffix(PROFILE_CONFIG_SUFFIX)?;
    (!profile_name.is_empty()).then(|| profile_name.to_string())
}

fn config_watch_paths(config_path: &Path, scope: ConfigWatchScope) -> io::Result<Vec<PathBuf>> {
    let fallback = PathBuf::from(".");
    let mut watch_paths = vec![config_path.parent().unwrap_or(&fallback).to_path_buf()];

    if scope == ConfigWatchScope::AllProfiles {
        if let Some(home_dir) = dirs::home_dir() {
            watch_paths.push(home_dir.join(".color-ssh"));
            watch_paths.push(home_dir);
        }
        watch_paths.push(env::current_dir()?);
    }

    watch_paths.sort();
    watch_paths.dedup();
    Ok(watch_paths)
}

fn classify_reload_events(event: &Event, config_path: &Path, active_profile: Option<&str>, scope: ConfigWatchScope) -> Vec<PendingReloadEvent> {
    let active_config_changed = should_reload_for_event(event, config_path);
    if !(event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove()) {
        return Vec::new();
    }

    let mut pending = Vec::new();
    if active_config_changed {
        pending.push(PendingReloadEvent::ActiveConfig);
    }

    if scope == ConfigWatchScope::AllProfiles {
        let mut profiles = HashSet::new();
        for path in &event.paths {
            let Some(profile_name) = profile_name_from_config_path(path) else {
                continue;
            };
            if active_profile == Some(profile_name.as_str()) {
                continue;
            }
            if profiles.insert(profile_name.clone()) {
                pending.push(PendingReloadEvent::Profile(profile_name));
            }
        }
    }

    pending
}

/// Start watching the active config file for changes.
pub fn config_watcher(profile: Option<String>, notice_target: ReloadNoticeTarget) -> Option<RecommendedWatcher> {
    config_watcher_with_scope(profile, notice_target, ConfigWatchScope::ActiveProfileOnly)
}

/// Start watching configuration files with an explicit watch scope.
pub fn config_watcher_with_scope(profile: Option<String>, notice_target: ReloadNoticeTarget, scope: ConfigWatchScope) -> Option<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel();

    log_debug!("Initializing configuration file watcher");

    let config_path = super::with_current_config("starting watcher", |cfg| cfg.metadata.config_path.clone());
    let config_path_clone = config_path.clone();
    let active_profile = profile.clone();

    let mut watcher = match RecommendedWatcher::new(
        move |res: Result<Event, Error>| {
            if let Ok(event) = res {
                let pending = classify_reload_events(&event, &config_path_clone, active_profile.as_deref(), scope);
                if !pending.is_empty() {
                    log_debug!("Config file change detected: {:?}", event);
                    for item in pending {
                        if tx.send(item).is_err() {
                            break;
                        }
                    }
                }
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

    let watch_paths = match config_watch_paths(&config_path, scope) {
        Ok(paths) => paths,
        Err(err) => {
            log_error!("Failed to compute config watch paths: {}", err);
            log_warn!("Configuration hot-reload disabled");
            return None;
        }
    };

    log_info!("Starting config watcher for: {:?} (watching: {:?})", config_path, watch_paths);

    for watch_path in &watch_paths {
        if let Err(err) = watcher.watch(watch_path, RecursiveMode::NonRecursive) {
            log_error!("Failed to watch config directory '{}': {}", watch_path.display(), err);
            log_warn!("Configuration hot-reload disabled");
            return None;
        }
    }

    if let Err(err) = thread::Builder::new().name("config-watcher".to_string()).spawn(move || {
        log_debug!("Config watcher thread started");
        loop {
            match rx.recv() {
                Ok(first_event) => {
                    // Debounce bursty editor writes before one reload attempt.
                    let mut reload_active_config = matches!(first_event, PendingReloadEvent::ActiveConfig);
                    let mut changed_profiles = HashSet::new();
                    if let PendingReloadEvent::Profile(profile_name) = first_event {
                        changed_profiles.insert(profile_name);
                    }

                    while let Ok(next_event) = rx.recv_timeout(Duration::from_millis(500)) {
                        match next_event {
                            PendingReloadEvent::ActiveConfig => reload_active_config = true,
                            PendingReloadEvent::Profile(profile_name) => {
                                changed_profiles.insert(profile_name);
                            }
                        }
                    }

                    if reload_active_config {
                        log_info!("Configuration change detected, reloading active config...");

                        let (message, success) = match ConfigLoader::new(profile.clone()) {
                            Ok(loader) => match loader.reload_config() {
                                Ok(()) => {
                                    log_info!("Configuration reloaded successfully");
                                    ("Config reloaded successfully".to_string(), true)
                                }
                                Err(err) => {
                                    log_error!("Error reloading config: {}", err);
                                    (format!("Config reload failed: {}", err), false)
                                }
                            },
                            Err(err) => {
                                log_error!("Error creating config loader for reload: {}", err);
                                (format!("Config reload failed: {}", err), false)
                            }
                        };

                        emit_reload_notice(&message, notice_target);
                        if let Some(active_profile) = profile.clone() {
                            queue_profile_reload_result(active_profile, success, message);
                        }
                    }

                    for profile_name in changed_profiles {
                        log_info!("Configuration change detected for profile '{}', validating...", profile_name);
                        let (message, success) = match ConfigLoader::new(Some(profile_name.clone())) {
                            Ok(loader) => match loader.load_config() {
                                Ok(_) => {
                                    log_info!("Configuration profile '{}' reloaded successfully", profile_name);
                                    (format!("Config profile '{}' reloaded successfully", profile_name), true)
                                }
                                Err(err) => {
                                    log_error!("Error reloading profile config '{}': {}", profile_name, err);
                                    (format!("Config profile '{}' reload failed: {}", profile_name, err), false)
                                }
                            },
                            Err(err) => {
                                log_error!("Error creating config loader for profile '{}': {}", profile_name, err);
                                (format!("Config profile '{}' reload failed: {}", profile_name, err), false)
                            }
                        };
                        queue_profile_reload_result(profile_name, success, message);
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
