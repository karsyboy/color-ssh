use super::migrate_ssh_config_to_inventory;
use crate::inventory::{ConnectionProtocol, InventoryHost, InventoryTreeModel, build_inventory_tree};
use crate::test::support::fs::TestWorkspace;
use std::fs;
use std::process::Command;

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

#[test]
fn migration_decodes_openssh_quoted_and_equals_form_values() {
    let workspace = TestWorkspace::new("inventory", "migration_syntax").expect("temp workspace");
    let ssh_config_path = workspace.join("config");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    let identity_path = workspace.join("keys/identity with spaces");

    workspace
        .write(
            &ssh_config_path,
            &format!(
                "Host quoted\nHostName \"example.com\" # inline comment\nUser=\"alice\"\nPort = 2222\nIdentityFile=\"{}\" # another comment\n",
                identity_path.display()
            ),
        )
        .expect("write ssh config");

    migrate_ssh_config_to_inventory(&ssh_config_path, &inventory_path).expect("migrate inventory");
    let tree = build_inventory_tree(&inventory_path).expect("load migrated inventory");
    let migrated = host_named(&tree, "quoted");

    let output = Command::new("ssh")
        .arg("-G")
        .arg("-F")
        .arg(&ssh_config_path)
        .arg("quoted")
        .output()
        .expect("run ssh -G");
    assert!(output.status.success(), "ssh -G failed: {}", String::from_utf8_lossy(&output.stderr));
    let effective = String::from_utf8(output.stdout).expect("ssh -G output is UTF-8");
    let effective_value = |key: &str| {
        effective
            .lines()
            .find_map(|line| line.strip_prefix(key).and_then(|value| value.strip_prefix(' ')))
            .unwrap_or_else(|| panic!("ssh -G did not emit {key}"))
    };

    assert_eq!(migrated.host, effective_value("hostname"));
    assert_eq!(migrated.user.as_deref(), Some(effective_value("user")));
    assert_eq!(migrated.port.map(|port| port.to_string()).as_deref(), Some(effective_value("port")));
    assert_eq!(migrated.ssh.identity_files.first().map(String::as_str), Some(effective_value("identityfile")));
}

#[test]
fn migration_rejects_reserved_include_stems_without_replacing_inventory() {
    let workspace = TestWorkspace::new("inventory", "migration_reserved_folder").expect("temp workspace");
    let original = "inventory:\n  - name: 'existing'\n    protocol: 'ssh'\n    host: 'existing.example'\n";

    for (index, include_name) in ["name.conf", "Name.conf", "na-me.conf", "n_a_m.e.conf"].iter().enumerate() {
        let ssh_config_path = workspace.join(&format!("config-{index}"));
        let inventory_path = workspace.join(&format!("cossh-inventory-{index}.yaml"));
        workspace
            .write(&ssh_config_path, &format!("Include conf.d/{include_name}\n"))
            .expect("write SSH config");
        workspace
            .write_rel(&format!("conf.d/{include_name}"), "Host migrated\nHostName migrated.example\n")
            .expect("write SSH include");
        workspace.write(&inventory_path, original).expect("write existing inventory");

        let err = migrate_ssh_config_to_inventory(&ssh_config_path, &inventory_path).expect_err("reserved include stem should fail migration");

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(fs::read_to_string(&inventory_path).expect("read inventory"), original);
        let tree = build_inventory_tree(&inventory_path).expect("reload unchanged inventory");
        assert_eq!(host_named(&tree, "existing").host, "existing.example");
    }
}
