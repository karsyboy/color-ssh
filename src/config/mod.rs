//! Runtime configuration loading, storage, and reload hooks.
//!
//! The active config lives in [`SESSION_CONFIG`] and is shared through a
//! process-wide lock.

mod errors;
mod loader;
mod paths;
mod schema;
mod watcher;

pub use errors::ConfigError;
pub use schema::{AuthSettings, Config, HighlightOverlayAutoPolicy, HighlightOverlayMode, HighlightRule, InteractiveSettings, Metadata, Settings};
pub use watcher::config_watcher;

use crate::highlight_rules::CompiledHighlightRule;
use once_cell::sync::OnceCell;
use regex::Regex;
use regex::RegexSet;
use std::io;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

/// Global configuration instance used by runtime components.
///
/// # Examples
///
/// ```no_run
/// use cossh::config;
///
/// // Read configuration using helper
/// let show_title = config::with_current_config("reading show_title", |cfg| cfg.settings.show_title);
///
/// // Write configuration using helper
/// config::with_current_config_mut("setting session name", |cfg| {
///     cfg.metadata.session_name = "example".to_string();
/// });
/// ```
pub static SESSION_CONFIG: OnceCell<Arc<RwLock<Config>>> = OnceCell::new();
static CONFIG_VERSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct InteractiveProfileSnapshot {
    pub(crate) auth_settings: AuthSettings,
    pub(crate) history_buffer: usize,
    pub(crate) remote_clipboard_write: bool,
    pub(crate) remote_clipboard_max_bytes: usize,
    pub(crate) ssh_logging_enabled: bool,
    pub(crate) secret_patterns: Vec<Regex>,
    pub(crate) overlay_rules: Vec<CompiledHighlightRule>,
    pub(crate) overlay_rule_set: Option<RegexSet>,
    pub(crate) overlay_mode: HighlightOverlayMode,
    pub(crate) overlay_auto_policy: HighlightOverlayAutoPolicy,
    pub(crate) config_version: u64,
}

fn fallback_config() -> Config {
    Config {
        settings: Settings::default(),
        auth_settings: AuthSettings::default(),
        interactive_settings: None,
        palette: std::collections::HashMap::new(),
        rules: Vec::new(),
        metadata: Metadata {
            session_name: "session".to_string(),
            ..Default::default()
        },
    }
}

fn replace_config(shared_config: &Arc<RwLock<Config>>, config: Config) {
    match shared_config.write() {
        Ok(mut guard) => *guard = config,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = config;
        }
    }
}

/// Get the global configuration container.
pub fn get_config() -> &'static Arc<RwLock<Config>> {
    SESSION_CONFIG.get_or_init(|| Arc::new(RwLock::new(fallback_config())))
}

/// Run a read-only closure against the current configuration.
///
/// If the lock is poisoned, this recovers the inner value and logs the context.
pub fn with_current_config<T>(context: &str, with_config: impl FnOnce(&Config) -> T) -> T {
    match get_config().read() {
        Ok(config_guard) => with_config(&config_guard),
        Err(poisoned) => {
            crate::log_error!("Configuration lock poisoned while {}; continuing with recovered state", context);
            let config_guard = poisoned.into_inner();
            with_config(&config_guard)
        }
    }
}

/// Run a mutable closure against the current configuration.
///
/// If the lock is poisoned, this recovers the inner value and logs the context.
pub fn with_current_config_mut<T>(context: &str, with_config: impl FnOnce(&mut Config) -> T) -> T {
    match get_config().write() {
        Ok(mut config_guard) => with_config(&mut config_guard),
        Err(poisoned) => {
            crate::log_error!("Configuration lock poisoned while {}; continuing with recovered state", context);
            let mut config_guard = poisoned.into_inner();
            with_config(&mut config_guard)
        }
    }
}

fn install_config(config: Config) {
    set_config_version(config.metadata.version);
    replace_config(get_config(), config);
}

fn snapshot_from_loaded_config(config: Config, config_version: u64) -> InteractiveProfileSnapshot {
    let Config {
        settings,
        auth_settings,
        interactive_settings,
        metadata,
        ..
    } = config;
    let interactive = interactive_settings.unwrap_or_default();
    InteractiveProfileSnapshot {
        auth_settings,
        history_buffer: interactive.history_buffer,
        remote_clipboard_write: interactive.allow_remote_clipboard_write,
        remote_clipboard_max_bytes: interactive.remote_clipboard_max_bytes,
        ssh_logging_enabled: settings.ssh_logging,
        secret_patterns: metadata.compiled_secret_patterns,
        overlay_rules: metadata.compiled_rules,
        overlay_rule_set: metadata.compiled_rule_set,
        overlay_mode: interactive.overlay_highlighting,
        overlay_auto_policy: interactive.overlay_auto_policy,
        config_version,
    }
}

fn current_interactive_profile_snapshot() -> InteractiveProfileSnapshot {
    with_current_config("reading interactive profile snapshot", |cfg| {
        let interactive = cfg.interactive_settings.as_ref();
        InteractiveProfileSnapshot {
            auth_settings: cfg.auth_settings.clone(),
            history_buffer: interactive.map(|interactive| interactive.history_buffer).unwrap_or(1000),
            remote_clipboard_write: interactive.map(|interactive| interactive.allow_remote_clipboard_write).unwrap_or(false),
            remote_clipboard_max_bytes: interactive.map(|interactive| interactive.remote_clipboard_max_bytes).unwrap_or(4096),
            ssh_logging_enabled: cfg.settings.ssh_logging,
            secret_patterns: cfg.metadata.compiled_secret_patterns.clone(),
            overlay_rules: cfg.metadata.compiled_rules.clone(),
            overlay_rule_set: cfg.metadata.compiled_rule_set.clone(),
            overlay_mode: interactive.map(|interactive| interactive.overlay_highlighting).unwrap_or_default(),
            overlay_auto_policy: interactive.map(|interactive| interactive.overlay_auto_policy).unwrap_or_default(),
            config_version: current_config_version(),
        }
    })
}

pub(crate) fn interactive_profile_snapshot(profile: Option<&str>) -> io::Result<InteractiveProfileSnapshot> {
    let profile = profile.map(str::trim).filter(|profile| !profile.is_empty());
    match profile {
        Some(profile_name) => {
            let config_loader = loader::ConfigLoader::new(Some(profile_name.to_string()))?;
            let config = config_loader.load_config()?;
            Ok(snapshot_from_loaded_config(config, 0))
        }
        None => Ok(current_interactive_profile_snapshot()),
    }
}

/// Load and install session configuration for an optional profile.
pub fn init_session_config(profile: Option<String>) -> Result<(), ConfigError> {
    let config_loader = loader::ConfigLoader::new(profile).map_err(ConfigError::IoError)?;
    let config = config_loader.load_config().map_err(ConfigError::IoError)?;
    install_config(config);
    Ok(())
}

/// Return auth settings from the currently active configuration.
pub fn auth_settings() -> AuthSettings {
    with_current_config("reading auth settings", |cfg| cfg.auth_settings.clone())
}

pub(crate) fn current_config_version() -> u64 {
    CONFIG_VERSION.load(Ordering::Acquire)
}

pub(crate) fn set_config_version(version: u64) {
    CONFIG_VERSION.store(version, Ordering::Release);
}

#[cfg(test)]
#[path = "../test/config.rs"]
mod tests;
