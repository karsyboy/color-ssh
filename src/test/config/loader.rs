use super::compile_secret_patterns;
use crate::config::{Config, HighlightOverlayAutoPolicy};
use crate::test::support::config::base_config;

#[test]
fn compile_secret_patterns_core_paths() {
    let mut config = base_config();

    config.settings.remove_secrets = Some(vec!["token=\\w+".to_string(), "[".to_string()]);
    let patterns = compile_secret_patterns(&config);
    assert_eq!(patterns.len(), 1);
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
