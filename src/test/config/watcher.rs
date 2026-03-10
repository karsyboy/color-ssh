use super::{ConfigWatchScope, PendingReloadEvent, classify_reload_events, should_reload_for_event};
use notify::{
    Event,
    event::{CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode},
};
use std::path::{Path, PathBuf};

fn event(kind: EventKind, paths: &[&str]) -> Event {
    Event {
        kind,
        paths: paths.iter().map(PathBuf::from).collect(),
        attrs: Default::default(),
    }
}

#[test]
fn should_reload_for_event_modify_or_create_target_file_returns_true() {
    let config_path = Path::new("/tmp/cossh-config.yaml");

    let modify_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/cossh-config.yaml"]);
    let create_event = event(EventKind::Create(CreateKind::Any), &["/tmp/cossh-config.yaml"]);
    let rename_event = event(
        EventKind::Modify(ModifyKind::Name(RenameMode::To)),
        &["/tmp/.cossh-config.yaml.tmp", "/tmp/cossh-config.yaml"],
    );
    let remove_event = event(EventKind::Remove(RemoveKind::Any), &["/tmp/cossh-config.yaml"]);
    let wrong_file = event(EventKind::Modify(ModifyKind::Any), &["/tmp/other.yaml"]);

    assert!(should_reload_for_event(&modify_event, config_path));
    assert!(should_reload_for_event(&create_event, config_path));
    assert!(should_reload_for_event(&rename_event, config_path));
    assert!(should_reload_for_event(&remove_event, config_path));
    assert!(!should_reload_for_event(&wrong_file, config_path));
}

#[test]
fn classify_reload_events_ignores_profile_files_when_scope_is_active_only() {
    let config_path = Path::new("/tmp/cossh-config.yaml");
    let profile_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/linux.cossh-config.yaml"]);

    let pending = classify_reload_events(&profile_event, config_path, None, ConfigWatchScope::ActiveProfileOnly);

    assert!(pending.is_empty());
}

#[test]
fn classify_reload_events_marks_non_active_profile_changes_for_tui_scope() {
    let config_path = Path::new("/tmp/cossh-config.yaml");
    let profile_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/linux.cossh-config.yaml"]);

    let pending = classify_reload_events(&profile_event, config_path, None, ConfigWatchScope::AllProfiles);

    assert_eq!(pending, vec![PendingReloadEvent::Profile("linux".to_string())]);
}

#[test]
fn classify_reload_events_keeps_active_profile_reloads_on_active_path_only() {
    let config_path = Path::new("/tmp/linux.cossh-config.yaml");
    let profile_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/linux.cossh-config.yaml"]);

    let pending = classify_reload_events(&profile_event, config_path, Some("linux"), ConfigWatchScope::AllProfiles);

    assert_eq!(pending, vec![PendingReloadEvent::ActiveConfig]);
}
