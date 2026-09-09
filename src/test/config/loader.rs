use super::compile_secret_patterns;
use crate::config::{AuthSettings, Config, HighlightOverlayAutoPolicy, InteractiveSettings};
use crate::test::support::config::base_config;

#[test]
fn compile_secret_patterns_skips_invalid_regex_entries() {
    let mut config = base_config();

    config.settings.remove_secrets = Some(vec!["token=\\w+".to_string(), "[".to_string()]);
    let patterns = compile_secret_patterns(&config);
    assert_eq!(patterns.len(), 1);
}

#[test]
fn config_default_auth_session_timeout_matches_safe_template_default() {
    let auth_settings = AuthSettings::default();

    assert_eq!(auth_settings.session_timeout_seconds, 3_600);
}

#[test]
fn config_schema_rejects_unknown_fields() {
    let yaml = r##"
settings: {}
interactive_settings: {}
palette:
  ok_fg: "#00ff00"
rules: []
unknown_top_level: true
"##;

    let err = serde_yml::from_str::<Config>(yaml).expect_err("unknown field should fail schema validation");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn config_schema_accepts_overlay_auto_policy_variants() {
    let yaml = r##"
settings: {}
interactive_settings:
    overlay_highlighting: auto
    overlay_auto_policy: reduced
palette:
    ok_fg: "#00ff00"
rules: []
"##;

    let config = serde_yml::from_str::<Config>(yaml).expect("overlay auto policy should deserialize");
    let interactive = config.interactive_settings.expect("interactive settings");
    assert_eq!(interactive.overlay_auto_policy, HighlightOverlayAutoPolicy::Reduced);
}

#[test]
fn omitted_interactive_settings_match_empty_block_defaults() {
    fn effective_settings(yaml: &str) -> InteractiveSettings {
        serde_yml::from_str::<Config>(yaml)
            .expect("config should deserialize")
            .interactive_settings
            .unwrap_or_default()
    }

    let omitted = effective_settings("palette: {}\nrules: []\n");
    let empty = effective_settings("interactive_settings: {}\npalette: {}\nrules: []\n");

    assert_eq!(omitted.history_buffer, 1000);
    assert_eq!(omitted.history_buffer, empty.history_buffer);
    assert_eq!(omitted.host_tree_uncollapsed, empty.host_tree_uncollapsed);
    assert_eq!(omitted.info_view, empty.info_view);
    assert_eq!(omitted.host_view_size, empty.host_view_size);
    assert_eq!(omitted.info_view_size, empty.info_view_size);
    assert_eq!(omitted.allow_remote_clipboard_write, empty.allow_remote_clipboard_write);
    assert_eq!(omitted.remote_clipboard_max_bytes, empty.remote_clipboard_max_bytes);
    assert_eq!(omitted.overlay_highlighting, empty.overlay_highlighting);
    assert_eq!(omitted.overlay_auto_policy, empty.overlay_auto_policy);
}
