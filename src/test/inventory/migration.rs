use super::migrate_ssh_config_to_inventory;
use crate::inventory::{ConnectionProtocol, InventoryHost, InventoryTreeModel, build_inventory_tree};
use crate::test::support::fs::TestWorkspace;

fn host_named<'a>(tree: &'a InventoryTreeModel, name: &str) -> &'a InventoryHost {
    tree.hosts
        .iter()
        .find(|host| host.name == name)
        .unwrap_or_else(|| panic!("missing host '{name}'"))
}

#[test]
fn migrate_ssh_config_to_inventory_writes_loadable_hosts_with_metadata() {
    let workspace = TestWorkspace::new("inventory", "migration").expect("temp workspace");
    let ssh_config_path = workspace.join("config");
    let inventory_path = workspace.join("cossh-inventory.yaml");

    workspace
        .write(
            &ssh_config_path,
            r#"
Host app-a app-b web-*
#_Profile prod
#_pass shared_key
HostName app.internal
User deploy
Port 2222

Host desktop01
#_Protocol rdp
#_RdpDomain ACME
#_RdpArgs /f +clipboard
#_hidden true
HostName rdp.internal
User administrator
Port 3390
"#,
        )
        .expect("write ssh config");

    let summary = migrate_ssh_config_to_inventory(&ssh_config_path, &inventory_path).expect("migrate inventory");

    assert_eq!(summary.hosts_written, 3);
    assert_eq!(summary.wildcard_aliases_skipped, 1);

    let tree = build_inventory_tree(&inventory_path).expect("load migrated inventory");
    assert_eq!(host_named(&tree, "app-a").protocol, ConnectionProtocol::Ssh);

    let desktop = host_named(&tree, "desktop01");
    assert_eq!(desktop.protocol, ConnectionProtocol::Rdp);
    assert_eq!(desktop.rdp.domain.as_deref(), Some("ACME"));
}
