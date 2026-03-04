//! Config file loading and compile pipeline.

use super::{Config, paths};
use crate::highlighter::CompiledHighlightRule;
use crate::{debug_enabled, log_debug, log_info, log_warn};
use regex::{Regex, RegexSet};
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

                let mut invalid_colors = Vec::new();
                for (color_name, value) in config.palette.iter() {
                    if !is_valid_hex_color(value) {
                        log_warn!("Invalid hex color '{}' for palette entry '{}'", value, color_name);
                        invalid_colors.push(color_name.clone());
                    }
                }

                if !invalid_colors.is_empty() {
                    log_warn!("Found {} invalid color(s): {:?}", invalid_colors.len(), invalid_colors);
                }

                let compiled_rules = compile_rules(&config);
                log_info!("Compiled {} highlight rules", compiled_rules.len());
                config.metadata.compiled_rule_set = compile_rule_set(&compiled_rules);
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
            // Preserve session naming used by active logging/session state.
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

/// Returns `true` for `#RRGGBB` values.
fn is_valid_hex_color(color: &str) -> bool {
    if color.len() != 7 || !color.starts_with('#') {
        return false;
    }
    color[1..].chars().all(|hex_char| hex_char.is_ascii_hexdigit())
}

/// Compile highlight rules into runtime regex/style pairs.
fn compile_rules(config: &Config) -> Vec<CompiledHighlightRule> {
    let mut rules = Vec::new();
    let mut failed_rules = Vec::new();
    let mut missing_colors = Vec::new();

    for (idx, rule) in config.rules.iter().enumerate() {
        let fg_color = match config.palette.get(&rule.color) {
            Some(hex) => hex_to_ansi(hex, ColorType::Foreground),
            None => {
                missing_colors.push((idx + 1, rule.color.clone()));
                String::new()
            }
        };

        let bg_color = if let Some(bg_name) = &rule.bg_color {
            match config.palette.get(bg_name) {
                Some(hex) => hex_to_ansi(hex, ColorType::Background),
                None => {
                    missing_colors.push((idx + 1, format!("{} (background)", bg_name)));
                    String::new()
                }
            }
        } else {
            String::new()
        };

        let ansi_code = if !fg_color.is_empty() && !bg_color.is_empty() {
            let fg_params = &fg_color[2..fg_color.len() - 1]; // Remove \x1b[ and m
            let bg_params = &bg_color[2..bg_color.len() - 1];
            format!("\x1b[{};{}m", fg_params, bg_params)
        } else if !fg_color.is_empty() {
            fg_color
        } else if !bg_color.is_empty() {
            bg_color
        } else {
            "\x1b[0m".to_string() // Reset if no valid colors
        };

        // Strip literal newlines from YAML block rules before regex compilation.
        let clean_regex = rule.regex.replace('\n', "").trim().to_string();

        match Regex::new(&clean_regex) {
            Ok(regex) => rules.push(CompiledHighlightRule::new(regex, ansi_code)),
            Err(err) => {
                log_warn!("Invalid regex in rule #{} ('{}'): {}", idx + 1, clean_regex, err);
                failed_rules.push((idx + 1, clean_regex));
            }
        }
    }

    if !missing_colors.is_empty() {
        log_warn!("Rules referencing missing palette colors: {:?}", missing_colors);
    }
    if !failed_rules.is_empty() {
        log_warn!("Failed to compile {} regex rule(s)", failed_rules.len());
    }

    if debug_enabled!() {
        for (i, rule) in rules.iter().enumerate() {
            log_debug!(
                "Rule {}: regex = {:?}, color = {:?}, reset_mode = {:?}",
                i + 1,
                rule.regex,
                rule.style,
                rule.reset_mode
            );
        }
    }

    rules
}

fn compile_rule_set(rules: &[CompiledHighlightRule]) -> Option<RegexSet> {
    if rules.is_empty() {
        return None;
    }

    let patterns: Vec<&str> = rules.iter().map(|rule| rule.regex.as_str()).collect();
    match RegexSet::new(patterns) {
        Ok(regex_set) => Some(regex_set),
        Err(err) => {
            log_warn!("Failed to compile regex prefilter set: {}", err);
            None
        }
    }
}

/// ANSI color channel type used by hex conversion.
#[derive(Debug, Clone, Copy)]
enum ColorType {
    Foreground,
    Background,
}

/// Convert `#RRGGBB` into a foreground/background ANSI escape sequence.
fn hex_to_ansi(hex: &str, color_type: ColorType) -> String {
    if hex.len() == 7
        && hex.starts_with('#')
        && let (Ok(red), Ok(green), Ok(blue)) = (
            u8::from_str_radix(&hex[1..3], 16),
            u8::from_str_radix(&hex[3..5], 16),
            u8::from_str_radix(&hex[5..7], 16),
        )
    {
        let code = match color_type {
            ColorType::Foreground => 38,
            ColorType::Background => 48,
        };
        return format!("\x1b[{};2;{};{};{}m", code, red, green, blue);
    }
    String::new()
}

/// Compile configured secret-redaction regex patterns.
fn compile_secret_patterns(config: &Config) -> Vec<Regex> {
    let mut patterns = Vec::new();

    if let Some(secret_strings) = &config.settings.remove_secrets {
        for (idx, pattern) in secret_strings.iter().enumerate() {
            match Regex::new(pattern) {
                Ok(regex) => patterns.push(regex),
                Err(err) => {
                    log_warn!("Failed to compile secret pattern #{}: '{}' - {}", idx + 1, pattern, err);
                }
            }
        }
    }

    patterns
}

#[cfg(test)]
#[path = "../test/config/loader.rs"]
mod tests;
