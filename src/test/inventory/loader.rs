use super::build_inventory_tree;
use crate::inventory::ConnectionProtocol;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_dir(name: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cossh_inventory_{name}_{nanos}_{serial}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

#[test]
fn parses_canonical_and_alias_inventory_keys() {
    let dir = test_dir("alias_keys").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");

    write_file(
        &inventory_path,
        r#"
inventory:
  - name: switch
    Description: Network Switch
    protocol: ssh
    HostName: 10.0.0.10
    user: admin
    port: "22"
    Vault_pass: shared
    IdentityFile: ~/.ssh/id_rsa
    IdentitiesOnly: yes
    ProxyJump: bastion
    ProxyCommand: ssh -W %h:%p bastion
    ForwardAgent: on
    LocalForward: 8080 localhost:80
    RemoteForward:
      - 9090 localhost:90
  - name: desktop01
    protocol: rdp
    host: 10.0.0.20
    user: administrator
    RdpDomain: ACME
    RdpArgs: /f +clipboard
"#,
    )
    .expect("write inventory");

    let tree = build_inventory_tree(&inventory_path).expect("load inventory");
    assert_eq!(tree.hosts.len(), 2);

    let switch = tree.hosts.iter().find(|host| host.name == "switch").expect("switch host");
    assert_eq!(switch.protocol, ConnectionProtocol::Ssh);
    assert_eq!(switch.description.as_deref(), Some("Network Switch"));
    assert_eq!(switch.host, "10.0.0.10");
    assert_eq!(switch.user.as_deref(), Some("admin"));
    assert_eq!(switch.port, Some(22));
    assert_eq!(switch.vault_pass.as_deref(), Some("shared"));
    assert_eq!(switch.ssh.identities_only, Some(true));
    assert_eq!(switch.ssh.proxy_jump.as_deref(), Some("bastion"));
    assert_eq!(switch.ssh.proxy_command.as_deref(), Some("ssh -W %h:%p bastion"));
    assert_eq!(switch.ssh.forward_agent.as_deref(), Some("yes"));
    assert_eq!(switch.ssh.local_forward, vec!["8080:localhost:80".to_string()]);
    assert_eq!(switch.ssh.remote_forward, vec!["9090:localhost:90".to_string()]);
    assert_eq!(switch.ssh.identity_files.len(), 1);
    assert!(switch.ssh.identity_files[0].ends_with(".ssh/id_rsa"));

    let desktop = tree.hosts.iter().find(|host| host.name == "desktop01").expect("desktop host");
    assert_eq!(desktop.protocol, ConnectionProtocol::Rdp);
    assert_eq!(desktop.rdp.domain.as_deref(), Some("ACME"));
    assert_eq!(desktop.rdp.args, vec!["/f".to_string(), "+clipboard".to_string()]);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn merges_same_named_folders_from_includes_and_keeps_hidden_hosts() {
    let dir = test_dir("merge_folders").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");
    write_file(
        &inventory_path,
        r#"
include:
  - ./inventory/*.yaml
inventory:
  - Shared:
      - name: hidden-a
        protocol: ssh
        host: 10.0.0.11
        hidden: true
"#,
    )
    .expect("write root inventory");
    write_file(
        &dir.join("inventory/10-extra.yaml"),
        r#"
inventory:
  - Shared:
      - name: visible-b
        protocol: ssh
        host: 10.0.0.12
"#,
    )
    .expect("write included inventory");

    let tree = build_inventory_tree(&inventory_path).expect("load inventory");
    assert_eq!(tree.hosts.len(), 2);
    assert!(tree.hosts.iter().any(|host| host.name == "hidden-a" && host.hidden));
    assert!(tree.hosts.iter().any(|host| host.name == "visible-b" && !host.hidden));

    let shared = tree.root.children.iter().find(|folder| folder.name == "Shared").expect("root shared folder");
    assert_eq!(shared.host_indices.len(), 1);

    let include_folder = tree.root.children.iter().find(|folder| folder.name == "10-extra").expect("include folder");
    assert_eq!(include_folder.children.len(), 1);
    assert_eq!(include_folder.children[0].name, "Shared");
    assert_eq!(include_folder.children[0].host_indices.len(), 1);

    let visible = tree.hosts.iter().find(|host| host.name == "visible-b").expect("visible-b");
    assert_eq!(visible.source_folder_path, vec!["10-extra".to_string(), "Shared".to_string()]);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejects_duplicate_host_names_after_include_merge() {
    let dir = test_dir("duplicate_hosts").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");
    write_file(
        &inventory_path,
        r#"
include:
  - ./extra.yaml
inventory:
  - name: switch
    protocol: ssh
    host: 10.0.0.10
"#,
    )
    .expect("write root inventory");
    write_file(
        &dir.join("extra.yaml"),
        r#"
inventory:
  - name: switch
    protocol: ssh
    host: 10.0.0.11
"#,
    )
    .expect("write duplicate inventory");

    let err = build_inventory_tree(&inventory_path).expect_err("duplicate host names should fail");
    assert!(err.to_string().contains("duplicate inventory host 'switch'"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn expands_glob_includes_in_sorted_order_and_handles_cycles() {
    let dir = test_dir("include_glob_cycle").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");
    write_file(
        &inventory_path,
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
    write_file(
        &dir.join("inventory/20-b.yaml"),
        r#"
inventory:
  - name: b
    protocol: ssh
    host: b.example
"#,
    )
    .expect("write b inventory");
    write_file(
        &dir.join("inventory/10-a.yaml"),
        format!(
            r#"
include:
  - {}
inventory:
  - name: a
    protocol: ssh
    host: a.example
"#,
            inventory_path.display()
        )
        .as_str(),
    )
    .expect("write a inventory");

    let tree = build_inventory_tree(&inventory_path).expect("load inventory");
    let names: Vec<&str> = tree.hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "root"]);
    assert_eq!(tree.root.children.len(), 2);
    assert_eq!(tree.root.children[0].name, "10-a");
    assert_eq!(tree.root.children[1].name, "20-b");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn nests_sub_includes_under_their_parent_include_folder() {
    let dir = test_dir("nested_include_folders").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");
    write_file(
        &inventory_path,
        r#"
include:
  - ./inventory/k-ops.yaml
inventory:
  - name: root
    protocol: ssh
    host: root.example
"#,
    )
    .expect("write root inventory");
    write_file(
        &dir.join("inventory/k-ops.yaml"),
        r#"
include:
  - ./regions/site-a.yaml
inventory:
  - name: jump
    protocol: ssh
    host: jump.example
"#,
    )
    .expect("write included inventory");
    write_file(
        &dir.join("inventory/regions/site-a.yaml"),
        r#"
inventory:
  - name: switch-a
    protocol: ssh
    host: switch-a.example
"#,
    )
    .expect("write nested inventory");

    let tree = build_inventory_tree(&inventory_path).expect("load inventory");
    assert_eq!(tree.root.children.len(), 1);
    assert_eq!(tree.root.children[0].name, "k-ops");
    assert_eq!(tree.root.children[0].host_indices.len(), 1);
    assert_eq!(tree.root.children[0].children.len(), 1);
    assert_eq!(tree.root.children[0].children[0].name, "site-a");
    assert_eq!(tree.root.children[0].children[0].host_indices.len(), 1);

    let jump = tree.hosts.iter().find(|host| host.name == "jump").expect("jump");
    assert_eq!(jump.source_folder_path, vec!["k-ops".to_string()]);

    let switch = tree.hosts.iter().find(|host| host.name == "switch-a").expect("switch-a");
    assert_eq!(switch.source_folder_path, vec!["k-ops".to_string(), "site-a".to_string()]);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parses_repeated_ssh_options_and_known_keys_inside_ssh_options_mapping() {
    let dir = test_dir("ssh_options_lists").expect("temp dir");
    let inventory_path = dir.join("cossh-inventory.yaml");

    write_file(
        &inventory_path,
        r#"
inventory:
  - name: jump
    protocol: ssh
    host: jump.example
    identity_file:
      - ~/.ssh/id_jump
      - ~/.ssh/id_ops
    ssh_options:
      ForwardAgent: $SSH_AUTH_SOCK
      CertificateFile:
        - ~/.ssh/id_jump-cert.pub
        - ~/.ssh/id_ops-cert.pub
      SendEnv:
        - LANG
        - LC_*
"#,
    )
    .expect("write inventory");

    let tree = build_inventory_tree(&inventory_path).expect("load inventory");
    let jump = tree.hosts.iter().find(|host| host.name == "jump").expect("jump");

    assert_eq!(jump.ssh.identity_files.len(), 2);
    assert!(jump.ssh.identity_files[0].ends_with(".ssh/id_jump"));
    assert!(jump.ssh.identity_files[1].ends_with(".ssh/id_ops"));
    assert_eq!(jump.ssh.forward_agent.as_deref(), Some("$SSH_AUTH_SOCK"));
    assert_eq!(
        jump.ssh.extra_options.get("CertificateFile"),
        Some(&vec!["~/.ssh/id_jump-cert.pub".to_string(), "~/.ssh/id_ops-cert.pub".to_string()])
    );
    assert_eq!(jump.ssh.extra_options.get("SendEnv"), Some(&vec!["LANG".to_string(), "LC_*".to_string()]));

    let _ = fs::remove_dir_all(dir);
}
