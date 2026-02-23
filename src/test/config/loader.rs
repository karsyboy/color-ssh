use super::{ColorType, compile_rule_set, compile_rules, compile_secret_patterns, hex_to_ansi, is_valid_hex_color};
use crate::config::style::{Config, HighlightRule, Metadata, Settings};
use std::collections::HashMap;

fn base_config() -> Config {
    Config {
        settings: Settings::default(),
        interactive_settings: None,
        palette: HashMap::new(),
        rules: Vec::new(),
        metadata: Metadata::default(),
    }
}

#[test]
fn validates_hex_color_format() {
    assert!(is_valid_hex_color("#00ffAA"));
    assert!(!is_valid_hex_color("00ffAA"));
    assert!(!is_valid_hex_color("#00ffA"));
    assert!(!is_valid_hex_color("#00ffZZ"));
}

#[test]
fn converts_hex_to_ansi_for_fg_and_bg() {
    assert_eq!(hex_to_ansi("#112233", ColorType::Foreground), "\x1b[38;2;17;34;51m");
    assert_eq!(hex_to_ansi("#112233", ColorType::Background), "\x1b[48;2;17;34;51m");
    assert_eq!(hex_to_ansi("oops", ColorType::Foreground), "");
}

#[test]
fn compiles_rules_and_handles_missing_colors_and_invalid_regex() {
    let mut config = base_config();
    config.palette.insert("ok_fg".to_string(), "#00ff00".to_string());
    config.palette.insert("ok_bg".to_string(), "#0000ff".to_string());
    config.rules = vec![
        HighlightRule {
            regex: "success".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: None,
        },
        HighlightRule {
            regex: "combo".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: Some("ok_bg".to_string()),
        },
        HighlightRule {
            regex: "fallback".to_string(),
            color: "missing".to_string(),
            description: None,
            bg_color: None,
        },
        HighlightRule {
            regex: "[unclosed".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: None,
        },
    ];

    let compiled = compile_rules(&config);
    assert_eq!(compiled.len(), 3, "invalid regex should be dropped");
    assert_eq!(compiled[0].style, "\x1b[38;2;0;255;0m");
    assert_eq!(compiled[1].style, "\x1b[38;2;0;255;0;48;2;0;0;255m");
    assert_eq!(compiled[2].style, "\x1b[0m", "missing palette entry should fall back to reset");
}

#[test]
fn compiles_rule_set_for_prefiltering() {
    let mut config = base_config();
    config.palette.insert("ok_fg".to_string(), "#00ff00".to_string());
    config.rules = vec![
        HighlightRule {
            regex: "error".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: None,
        },
        HighlightRule {
            regex: "warn".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: None,
        },
    ];

    let compiled_rules = compile_rules(&config);
    let rule_set = compile_rule_set(&compiled_rules).expect("rule set should compile");
    let matches = rule_set.matches("warn only");

    assert!(matches.matched(1));
    assert!(!matches.matched(0));
}

#[test]
fn compiles_only_valid_secret_patterns() {
    let mut config = base_config();
    config.settings.remove_secrets = Some(vec!["token=\\w+".to_string(), "[".to_string()]);

    let patterns = compile_secret_patterns(&config);
    assert_eq!(patterns.len(), 1);
    assert!(patterns[0].is_match("token=abc123"));
}

#[test]
fn rejects_unknown_top_level_config_fields() {
    let yaml = r##"
settings: {}
interactive_settings: {}
palette:
  ok_fg: "#00ff00"
rules: []
unknown_top_level: true
"##;

    let err = serde_yaml::from_str::<Config>(yaml).expect_err("unknown field should fail schema validation");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn rejects_unknown_interactive_settings_fields() {
    let yaml = r##"
settings: {}
interactive_settings:
  host_tree_uncollapsed: false
  unknown_interactive: true
palette:
  ok_fg: "#00ff00"
rules: []
"##;

    let err = serde_yaml::from_str::<Config>(yaml).expect_err("unknown interactive field should fail schema validation");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn accepts_rule_description_field() {
    let yaml = r##"
settings: {}
interactive_settings: {}
palette:
  ok_fg: "#00ff00"
rules:
  - regex: "error"
    color: "ok_fg"
    description: "Highlight errors in red"
"##;

    let parsed = serde_yaml::from_str::<Config>(yaml).expect("description should be accepted");
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(parsed.rules[0].description.as_deref(), Some("Highlight errors in red"));
    assert_eq!(parsed.rules[0].bg_color, None);
}
