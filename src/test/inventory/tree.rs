use super::build_inventory_tree;
use crate::inventory::{ConnectionProtocol, InventoryHost, InventoryTreeModel};
use crate::test::support::fs::TestWorkspace;
use std::path::{Path, PathBuf};

struct InventoryFixture {
    workspace: TestWorkspace,
    inventory_path: PathBuf,
}

impl InventoryFixture {
    fn new(prefix: &str, root_inventory_yaml: &str) -> Self {
        let workspace = TestWorkspace::new("inventory", prefix).expect("temp workspace");
        let inventory_path = workspace.join("cossh-inventory.yaml");
        workspace.write(&inventory_path, root_inventory_yaml).expect("write root inventory");

        Self { workspace, inventory_path }
    }

    fn root_path(&self) -> &Path {
        &self.inventory_path
    }

    fn write_rel(&self, rel: &str, contents: &str) {
        self.workspace.write_rel(rel, contents).expect("write fixture file");
    }

    fn load(&self) -> InventoryTreeModel {
        build_inventory_tree(self.root_path()).expect("load inventory")
    }

    fn load_err_message(&self) -> String {
        build_inventory_tree(self.root_path()).expect_err("inventory load should fail").to_string()
    }
}

fn host_named<'a>(tree: &'a InventoryTreeModel, name: &str) -> &'a InventoryHost {
    tree.hosts
        .iter()
        .find(|host| host.name == name)
        .unwrap_or_else(|| panic!("missing host '{name}'"))
}

#[test]
fn build_inventory_tree_canonical_aliases_and_protocol_fields_maps_to_host_model() {
    let fixture = InventoryFixture::new(
        "parse",
        r#"
inventory:
  - name: switch
    protocol: ssh
    HostName: 10.0.0.10
    user: admin
    port: "22"
    Vault_pass: shared
  - name: desktop01
    protocol: rdp
    host: 10.0.0.20
    RdpDomain: ACME
"#,
    );

    let tree = fixture.load();

    assert_eq!(tree.hosts.len(), 2);
    let switch = host_named(&tree, "switch");
    assert_eq!(switch.protocol, ConnectionProtocol::Ssh);
    assert_eq!(switch.host, "10.0.0.10");
    assert_eq!(switch.vault_pass.as_deref(), Some("shared"));

    let desktop = host_named(&tree, "desktop01");
    assert_eq!(desktop.protocol, ConnectionProtocol::Rdp);
    assert_eq!(desktop.rdp.domain.as_deref(), Some("ACME"));
}

#[test]
fn build_inventory_tree_includes_and_duplicate_hosts_merges_then_rejects_duplicates() {
    let fixture = InventoryFixture::new(
        "includes",
        r#"
include:
  - ./extra.yaml
inventory:
  - Shared:
      - name: hidden-a
        protocol: ssh
        host: 10.0.0.11
        hidden: true
"#,
    );
    fixture.write_rel(
        "extra.yaml",
        r#"
inventory:
  - Shared:
      - name: visible-b
        protocol: ssh
        host: 10.0.0.12
"#,
    );

    let merged_tree = fixture.load();
    assert!(merged_tree.hosts.iter().any(|host| host.name == "hidden-a"));
    assert!(merged_tree.hosts.iter().any(|host| host.name == "visible-b"));

    fixture.write_rel(
        "extra.yaml",
        r#"
inventory:
  - name: hidden-a
    protocol: ssh
    host: 10.0.0.99
"#,
    );

    assert!(fixture.load_err_message().contains("duplicate inventory host 'hidden-a'"));
}

#[test]
fn build_inventory_tree_glob_include_order_and_cycles_loads_each_file_once_in_sorted_order() {
    let fixture = InventoryFixture::new(
        "glob_cycle",
        r#"
include:
  - ./inventory/*.yaml
inventory:
  - name: root
    protocol: ssh
    host: root.example
"#,
    );
    fixture.write_rel(
        "inventory/20-b.yaml",
        r#"
inventory:
  - name: b
    protocol: ssh
    host: b.example
"#,
    );
    fixture.write_rel(
        "inventory/10-a.yaml",
        format!(
            r#"
include:
  - {}
inventory:
  - name: a
    protocol: ssh
    host: a.example
"#,
            fixture.root_path().display()
        )
        .as_str(),
    );

    let tree = fixture.load();
    let host_names: Vec<&str> = tree.hosts.iter().map(|host| host.name.as_str()).collect();

    assert_eq!(host_names, vec!["a", "b", "root"]);
}
