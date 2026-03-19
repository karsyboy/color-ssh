use super::{
    EditableInventoryHost, create_inventory_folder, create_inventory_host_entry, delete_inventory_folder, delete_inventory_host_entry,
    move_inventory_host_entry, rename_inventory_folder, update_inventory_host_entry,
};
use crate::inventory::{ConnectionProtocol, build_inventory_tree};
use crate::test::support::fs::TestWorkspace;
use std::collections::BTreeMap;
use std::fs;

fn editable_host(name: &str, host: &str) -> EditableInventoryHost {
    EditableInventoryHost {
        name: name.to_string(),
        host: host.to_string(),
        protocol: ConnectionProtocol::Ssh,
        ..EditableInventoryHost::default()
    }
}

#[test]
fn update_inventory_host_entry_updates_known_fields_and_preserves_unknown_custom_keys() {
    let workspace = TestWorkspace::new("inventory", "edit_update").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: old.example
    custom_keep: still-here
    ssh_options:
      StrictHostKeyChecking: no
"#,
        )
        .expect("write inventory");

    let mut updated = editable_host("alpha", "new.example");
    updated.user = Some("admin".to_string());
    updated.vault_pass = Some("shared".to_string());
    updated.hidden = true;
    updated.ssh_options = BTreeMap::from([("StrictHostKeyChecking".to_string(), vec!["ask".to_string()])]);

    update_inventory_host_entry(&inventory_path, "alpha", &updated).expect("update inventory host entry");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    let host = tree.hosts.iter().find(|host| host.name == "alpha").expect("updated host");

    assert_eq!(host.host, "new.example");
    assert_eq!(host.user.as_deref(), Some("admin"));
    assert_eq!(host.vault_pass.as_deref(), Some("shared"));
    assert!(host.hidden);
    assert_eq!(host.ssh.extra_options.get("StrictHostKeyChecking"), Some(&vec!["ask".to_string()]));

    let rendered = fs::read_to_string(&inventory_path).expect("read updated inventory");
    assert!(rendered.contains("custom_keep: still-here"));
}

#[test]
fn delete_inventory_host_entry_removes_host_recursively_from_nested_folder() {
    let workspace = TestWorkspace::new("inventory", "edit_delete").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Group:
      - name: alpha
        protocol: ssh
        host: alpha.example
      - name: beta
        protocol: ssh
        host: beta.example
"#,
        )
        .expect("write inventory");

    delete_inventory_host_entry(&inventory_path, "alpha").expect("delete inventory host entry");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    assert_eq!(tree.hosts.len(), 1);
    assert_eq!(tree.hosts[0].name, "beta");
}

#[test]
fn create_inventory_host_entry_creates_nested_folders_from_folder_path() {
    let workspace = TestWorkspace::new("inventory", "edit_create_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: existing
    protocol: ssh
    host: existing.example
"#,
        )
        .expect("write inventory");

    let mut created = editable_host("new-host", "new.example");
    created.protocol = ConnectionProtocol::Rdp;
    created.user = Some("alice".to_string());
    created.rdp_domain = Some("ACME".to_string());
    created.rdp_args = vec!["/cert:ignore".to_string()];

    create_inventory_host_entry(&inventory_path, &["folder".to_string(), "folder1".to_string()], &created).expect("create inventory host entry");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    let host = tree.hosts.iter().find(|host| host.name == "new-host").expect("created host");

    assert_eq!(host.host, "new.example");
    assert_eq!(host.source_folder_path, vec!["folder".to_string(), "folder1".to_string()]);
    assert_eq!(host.rdp.domain.as_deref(), Some("ACME"));

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("folder:"));
    assert!(rendered.contains("folder1:"));
}

#[test]
fn create_inventory_folder_creates_folder_in_nested_parent_path() {
    let workspace = TestWorkspace::new("inventory", "edit_create_inventory_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Team:
      - name: alpha
        protocol: ssh
        host: alpha.example
"#,
        )
        .expect("write inventory");

    create_inventory_folder(&inventory_path, &["Team".to_string()], "NewFolder").expect("create folder");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    assert!(
        tree.root
            .children
            .iter()
            .find(|folder| folder.name == "Team")
            .is_some_and(|team| team.children.iter().any(|child| child.name == "NewFolder"))
    );

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("Team:"));
    assert!(rendered.contains("NewFolder:"));
}

#[test]
fn create_inventory_folder_rejects_duplicate_name_under_same_parent() {
    let workspace = TestWorkspace::new("inventory", "edit_create_inventory_folder_duplicate").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Team:
      - Existing:
          - name: alpha
            protocol: ssh
            host: alpha.example
"#,
        )
        .expect("write inventory");

    let err = create_inventory_folder(&inventory_path, &["Team".to_string()], "Existing").expect_err("duplicate folder should fail");
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
}

#[test]
fn move_inventory_host_entry_moves_host_between_folders_in_single_write_flow() {
    let workspace = TestWorkspace::new("inventory", "edit_move_host").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - GroupA:
      - name: alpha
        protocol: ssh
        host: alpha.example
  - GroupB:
      - name: beta
        protocol: ssh
        host: beta.example
"#,
        )
        .expect("write inventory");

    move_inventory_host_entry(&inventory_path, "alpha", &["GroupB".to_string()]).expect("move inventory host entry");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    let moved = tree.hosts.iter().find(|host| host.name == "alpha").expect("moved host");
    assert_eq!(moved.source_folder_path, vec!["GroupB".to_string()]);

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    let group_a_index = rendered.find("GroupA:").expect("group a exists");
    let group_b_index = rendered.find("GroupB:").expect("group b exists");
    let alpha_index = rendered.find("name: alpha").expect("alpha exists");
    assert!(group_b_index < alpha_index, "alpha should be under GroupB after move");
    assert!(group_a_index < group_b_index, "GroupA should remain before GroupB in this fixture");
}

#[test]
fn rename_inventory_folder_updates_folder_key() {
    let workspace = TestWorkspace::new("inventory", "edit_rename_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - old-folder:
      - name: alpha
        protocol: ssh
        host: alpha.example
"#,
        )
        .expect("write inventory");

    rename_inventory_folder(&inventory_path, &["old-folder".to_string()], "new-folder").expect("rename folder");

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    let host = tree.hosts.iter().find(|item| item.name == "alpha").expect("host");
    assert_eq!(host.source_folder_path, vec!["new-folder".to_string()]);

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("new-folder:"));
    assert!(!rendered.contains("old-folder:"));
}

#[test]
fn delete_inventory_folder_removes_descendants_and_returns_host_count() {
    let workspace = TestWorkspace::new("inventory", "edit_delete_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Keep:
      - name: keep-host
        protocol: ssh
        host: keep.example
  - Remove:
      - name: alpha
        protocol: ssh
        host: alpha.example
      - Nested:
          - name: beta
            protocol: ssh
            host: beta.example
"#,
        )
        .expect("write inventory");

    let removed = delete_inventory_folder(&inventory_path, &["Remove".to_string()]).expect("delete folder");
    assert_eq!(removed, 2);

    let tree = build_inventory_tree(&inventory_path).expect("reload inventory");
    assert!(tree.hosts.iter().any(|host| host.name == "keep-host"));
    assert!(tree.hosts.iter().all(|host| host.name != "alpha"));
    assert!(tree.hosts.iter().all(|host| host.name != "beta"));

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("Keep:"));
    assert!(!rendered.contains("Remove:"));
}
