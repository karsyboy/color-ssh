//! Configuration file path discovery and default-file bootstrap.

use crate::{args, log_debug, log_info, log_warn};
use std::{env, fs, io, path::PathBuf};

pub(crate) const DEFAULT_CONFIG_FILENAME: &str = "cossh-config.yaml";

pub(crate) fn resolve_config_path(profile: Option<&str>) -> io::Result<PathBuf> {
    let normalized_profile = normalize_profile_name(profile)?;
    let config_filename = config_filename(normalized_profile.as_deref());

    for candidate in config_candidates(&config_filename)? {
        log_debug!("Checking config path: {:?}", candidate);
        if candidate.exists() {
            log_info!("Found config at: {:?}", candidate);
            return Ok(candidate);
        }
    }

    if let Some(profile_name) = normalized_profile {
        let message = format!(
            "Configuration profile '{}' not found. Please ensure the file exists in one of the standard locations.",
            profile_name
        );
        log_warn!("{}", message);
        return Err(io::Error::new(io::ErrorKind::NotFound, message));
    }

    log_warn!("No config file found, creating default configuration");
    create_default_config()
}

fn normalize_profile_name(profile: Option<&str>) -> io::Result<Option<String>> {
    match profile.map(str::trim) {
        Some("") | None => Ok(None),
        Some(profile_name) if args::validate_profile_name(profile_name) => Ok(Some(profile_name.to_string())),
        Some(profile_name) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid profile name '{}': use only letters, numbers, '_' or '-'", profile_name),
        )),
    }
}

fn config_filename(profile: Option<&str>) -> String {
    match profile {
        Some(profile_name) => format!("{profile_name}.cossh-config.yaml"),
        None => DEFAULT_CONFIG_FILENAME.to_string(),
    }
}

fn config_candidates(config_filename: &str) -> io::Result<Vec<PathBuf>> {
    let mut candidates = Vec::with_capacity(3);

    if let Some(home_dir) = dirs::home_dir() {
        candidates.push(home_dir.join(".color-ssh").join(config_filename));
        candidates.push(home_dir.join(config_filename));
    }

    let current_dir = env::current_dir().map_err(|err| {
        log_warn!("Failed to get current directory: {}", err);
        io::Error::new(io::ErrorKind::NotFound, format!("Failed to get current directory: {err}"))
    })?;
    candidates.push(current_dir.join(config_filename));

    Ok(candidates)
}

pub(crate) fn create_default_config() -> io::Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Failed to get home directory"))?;
    let config_dir = home_dir.join(".color-ssh");
    let config_path = config_dir.join(DEFAULT_CONFIG_FILENAME);

    if !config_dir.exists() {
        log_debug!("Creating config directory: {:?}", config_dir);
        fs::create_dir(&config_dir)?;
    }

    let config_content = include_str!("../../templates/default.cossh-config.yaml");
    fs::write(&config_path, config_content)?;
    log_info!("Default configuration file created at: {:?}", config_path);

    Ok(config_path)
}
