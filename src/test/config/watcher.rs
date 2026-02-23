use super::should_reload_for_event;
use notify::{
    Event,
    event::{CreateKind, EventKind, ModifyKind, RemoveKind},
};
use std::path::PathBuf;

fn event(kind: EventKind, paths: &[&str]) -> Event {
    Event {
        kind,
        paths: paths.iter().map(PathBuf::from).collect(),
        attrs: Default::default(),
    }
}

#[test]
fn reloads_only_for_modify_or_create_on_target_file() {
    let config_name = "cossh-config.yaml";
    let modify_event = event(EventKind::Modify(ModifyKind::Any), &["/tmp/cossh-config.yaml"]);
    let create_event = event(EventKind::Create(CreateKind::Any), &["/tmp/cossh-config.yaml"]);
    let wrong_file = event(EventKind::Modify(ModifyKind::Any), &["/tmp/other.yaml"]);
    let remove_event = event(EventKind::Remove(RemoveKind::Any), &["/tmp/cossh-config.yaml"]);

    assert!(should_reload_for_event(&modify_event, config_name));
    assert!(should_reload_for_event(&create_event, config_name));
    assert!(!should_reload_for_event(&wrong_file, config_name));
    assert!(!should_reload_for_event(&remove_event, config_name));
}
