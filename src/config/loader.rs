//! Config file loading and compile pipeline.

use super::{Config, highlight, paths};
use crate::{log_debug, log_info, log_warn};
use regex::Regex;
use std::{fs, io, path::PathBuf};

pub(crate) struct ConfigLoader {
    config_path: PathBuf,
}

impl ConfigLoader {
    pub(crate) fn new(profile: Option<String>) -> Result<Self, io::Error> {
        let config_path = paths::resolve_config_path(profile.as_deref())?;
        Ok(Self { config_path })
    }

    /// Load, validate, and compile a config file.
    pub(crate) fn load_config(self) -> io::Result<Config> {
        log_info!("Loading configuration from: {:?}", self.config_path);

        let config_content = fs::read_to_string(self.config_path.clone()).map_err(|err| {
            log_warn!("Failed to read config file: {}", err);
            err
        })?;

        match serde_yml::from_str::<Config>(&config_content) {
            Ok(mut config) => {
                config.metadata.config_path = self.config_path;
                log_debug!("Parsed configuration successfully");

                let invalid_colors: Vec<String> = config
                    .palette
                    .iter()
                    .filter(|(_, value)| !highlight::is_valid_hex_color(value))
                    .map(|(name, value)| {
                        log_warn!("Invalid hex color '{}' for palette entry '{}'; removing from palette", value, name);
                        name.clone()
                    })
                    .collect();

                for name in &invalid_colors {
                    config.palette.remove(name);
                }

                if !invalid_colors.is_empty() {
                    log_warn!("Removed {} invalid palette color(s): {:?}", invalid_colors.len(), invalid_colors);
                }

                let compiled_rules = highlight::compile_rules(&config);
                log_info!("Compiled {} highlight rules", compiled_rules.len());
                config.metadata.compiled_rule_set = highlight::compile_rule_set(&compiled_rules);
                config.metadata.compiled_rules = compiled_rules;

                let compiled_secrets = compile_secret_patterns(&config);
                if !compiled_secrets.is_empty() {
                    log_info!("Compiled {} secret redaction patterns", compiled_secrets.len());
                }
                config.metadata.compiled_secret_patterns = compiled_secrets;

                Ok(config)
            }
            Err(err) => {
                log_warn!("Error parsing configuration file: {:?}", err);
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Error parsing configuration file: {:?}", err),
                ))
            }
        }
    }

    /// Reload and apply configuration into the shared config store.
    pub(crate) fn reload_config(self) -> Result<(), String> {
        log_info!("Reloading configuration...");
        let mut new_config = self.load_config().map_err(|err| {
            log_warn!("Failed to reload configuration: {}", err);
            err.to_string()
        })?;

        let (rule_count, secret_count, config_version) = super::with_current_config_mut("reloading configuration", |current_config| {
            // Preserve the active global SSH log filename stem across config reloads.
            new_config.metadata.session_name = current_config.metadata.session_name.clone();
            // Bump config version so active workers can detect reload.
            new_config.metadata.version = current_config.metadata.version.wrapping_add(1);

            *current_config = new_config;

            (
                current_config.metadata.compiled_rules.len(),
                current_config.metadata.compiled_secret_patterns.len(),
                current_config.metadata.version,
            )
        });

        log_info!("Reloaded {} highlight rules", rule_count);
        if secret_count > 0 {
            log_info!("Reloaded {} secret redaction patterns", secret_count);
        }

        super::set_config_version(config_version);
        log_info!("Configuration reloaded successfully (version {})", config_version);

        Ok(())
    }
}

/// Compile configured secret-redaction regex patterns.
fn compile_secret_patterns(config: &Config) -> Vec<Regex> {
    let mut patterns = Vec::new();

    if let Some(secret_strings) = &config.settings.remove_secrets {
        for (idx, pattern) in secret_strings.iter().enumerate() {
            match Regex::new(pattern) {
                Ok(regex) => patterns.push(regex),
                Err(err) => {
                    log_warn!(
                        "Secret redaction pattern #{} ('{}') is invalid and will be skipped: {}. Secrets matching this pattern will NOT be redacted.",
                        idx + 1,
                        pattern,
                        err
                    );
                }
            }
        }
    }

    patterns
}

#[cfg(test)]
#[path = "../test/config/loader.rs"]
mod tests;
