//! Highlight rule compilation and shared compiled-rule types.

use super::Config;
use crate::{debug_enabled, log_debug, log_warn};
use regex::{Regex, RegexSet};

#[derive(Debug, Clone)]
pub(crate) struct CompiledHighlightRule {
    pub(crate) regex: Regex,
    pub(crate) ansi_style: String,
}

impl CompiledHighlightRule {
    pub(crate) fn new(regex: Regex, ansi_style: String) -> Self {
        Self { regex, ansi_style }
    }
}

pub(super) fn compile_rules(config: &Config) -> Vec<CompiledHighlightRule> {
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

        let ansi_style = if !fg_color.is_empty() && !bg_color.is_empty() {
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

        let clean_regex = normalize_rule_regex(&rule.regex);

        match Regex::new(&clean_regex) {
            Ok(regex) => rules.push(CompiledHighlightRule::new(regex, ansi_style)),
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
            log_debug!("Rule {}: regex = {:?}, ansi_style = {:?}", i + 1, rule.regex, rule.ansi_style,);
        }
    }

    rules
}

pub(super) fn compile_rule_set(rules: &[CompiledHighlightRule]) -> Option<RegexSet> {
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

fn normalize_rule_regex(regex: &str) -> String {
    let trimmed = regex.trim();
    if has_global_extended_flag(trimmed) {
        trimmed.to_string()
    } else {
        trimmed.replace('\n', "")
    }
}

fn has_global_extended_flag(regex: &str) -> bool {
    let Some(rest) = regex.strip_prefix("(?") else {
        return false;
    };

    let Some(flags_end) = rest.find(')') else {
        return false;
    };

    let flags = &rest[..flags_end];
    if flags.is_empty() || flags.contains(':') {
        return false;
    }

    flags.split('-').next().is_some_and(|enabled| enabled.contains('x'))
}

pub(super) fn is_valid_hex_color(color: &str) -> bool {
    if color.len() != 7 || !color.starts_with('#') {
        return false;
    }
    color[1..].chars().all(|hex_char| hex_char.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ColorType {
    Foreground,
    Background,
}

pub(super) fn hex_to_ansi(hex: &str, color_type: ColorType) -> String {
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

#[cfg(test)]
#[path = "../test/config/highlight.rs"]
mod tests;
