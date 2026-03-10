use super::{build_inventory_watch_plan, should_reload_for_inventory_event};
use crate::test::support::fs::TestWorkspace;
use notify::{
    Event,
    event::{CreateKind, EventKind, ModifyKind},
};
use std::path::PathBuf;

fn event(kind: EventKind, paths: &[PathBuf]) -> Event {
    Event {
        kind,
        paths: paths.to_vec(),
        attrs: Default::default(),
    }
}

#[test]
fn build_inventory_watch_plan_collects_tracked_files_and_include_directories() {
    let workspace = TestWorkspace::new("inventory", "watch_plan").expect("temp workspace");
    let root = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &root,
            r#"
include:
  - ./inventory/*.yaml
  - ./extra.yaml
inventory:
  - name: root
    protocol: ssh
    host: root.example
"#,
        )
        .expect("write root inventory");
    let glob_file = workspace
        .write_rel(
            "inventory/10-a.yaml",
            r#"
inventory:
  - name: a
    protocol: ssh
    host: a.example
"#,
        )
        .expect("write glob include");
    let extra_file = workspace
        .write_rel(
            "extra.yaml",
            r#"
inventory:
  - name: extra
    protocol: ssh
    host: extra.example
"#,
        )
        .expect("write direct include");

    let plan = build_inventory_watch_plan(&root).expect("build watch plan");

    assert!(plan.tracked_files.contains(&root.canonicalize().expect("canonical root")));
    assert!(plan.tracked_files.contains(&glob_file.canonicalize().expect("canonical glob file")));
    assert!(plan.tracked_files.contains(&extra_file.canonicalize().expect("canonical direct include")));
    assert!(
        plan.include_dirs
            .contains(&workspace.join("inventory").canonicalize().expect("canonical include dir"))
    );
    assert!(plan.watch_paths.contains(&workspace.join("").canonicalize().expect("canonical workspace root")));
}

#[test]
fn should_reload_for_inventory_event_matches_tracked_files_and_yaml_creates_in_include_dirs() {
    let workspace = TestWorkspace::new("inventory", "watch_filter").expect("temp workspace");
    let root = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &root,
            r#"
include:
  - ./inventory/*.yaml
inventory:
  - name: root
    protocol: ssh
    host: root.example
"#,
        )
        .expect("write root inventory");
    workspace
        .write_rel(
            "inventory/10-a.yaml",
            r#"
inventory:
  - name: a
    protocol: ssh
    host: a.example
"#,
        )
        .expect("write include file");

    let plan = build_inventory_watch_plan(&root).expect("build watch plan");

    let root_modify = event(EventKind::Modify(ModifyKind::Any), std::slice::from_ref(&root));
    let new_include = event(EventKind::Create(CreateKind::Any), &[workspace.join("inventory/20-b.yaml")]);
    let unrelated_yaml = event(EventKind::Create(CreateKind::Any), &[workspace.join("other.yaml")]);
    let unrelated_text = event(EventKind::Create(CreateKind::Any), &[workspace.join("inventory/readme.txt")]);

    assert!(should_reload_for_inventory_event(&root_modify, &plan));
    assert!(should_reload_for_inventory_event(&new_include, &plan));
    assert!(!should_reload_for_inventory_event(&unrelated_yaml, &plan));
    assert!(!should_reload_for_inventory_event(&unrelated_text, &plan));
}
