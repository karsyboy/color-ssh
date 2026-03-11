use super::{ColorType, compile_rule_set, compile_rules, hex_to_ansi, is_valid_hex_color};
use crate::config::HighlightRule;
use crate::test::support::config::base_config;

#[test]
fn color_parsing_and_hex_validation_handle_valid_and_invalid_inputs() {
    assert!(is_valid_hex_color("#00ffAA"));
    assert!(!is_valid_hex_color("#00ffZZ"));
    assert_eq!(hex_to_ansi("#112233", ColorType::Foreground), "\x1b[38;2;17;34;51m");
    assert_eq!(hex_to_ansi("oops", ColorType::Foreground), "");
}

#[test]
fn compile_rules_drops_invalid_regex_and_falls_back_for_missing_palette() {
    let mut config = base_config();
    config.palette.insert("ok_fg".to_string(), "#00ff00".to_string());
    config.rules = vec![
        HighlightRule {
            regex: "success".to_string(),
            color: "ok_fg".to_string(),
            description: None,
            bg_color: None,
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
    assert_eq!(compiled.len(), 2);
    assert_eq!(compiled[0].ansi_style, "\x1b[38;2;0;255;0m");
    assert_eq!(compiled[1].ansi_style, "\x1b[0m");
}

#[test]
fn compile_rule_set_matches_compiled_patterns() {
    let mut config = base_config();
    config.palette.insert("ok_fg".to_string(), "#00ff00".to_string());
    config.rules = vec![HighlightRule {
        regex: "error".to_string(),
        color: "ok_fg".to_string(),
        description: None,
        bg_color: None,
    }];

    let compiled_rules = compile_rules(&config);
    let rule_set = compile_rule_set(&compiled_rules).expect("rule set should compile");
    assert!(rule_set.matches("error").matched(0));
}
