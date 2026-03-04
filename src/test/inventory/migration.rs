use super::migrate_ssh_config_to_inventory;
use crate::inventory::{ConnectionProtocol, build_inventory_tree};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_dir(name: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cossh_inventory_migration_{name}_{nanos}_{serial}"));
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
fn migrates_supported_ssh_fields_to_canonical_inventory_yaml() {
    let dir = test_dir("canonical").expect("temp dir");
    let ssh_config = dir.join("config");
    let inventory_path = dir.join("cossh-inventory.yaml");
    write_file(
        &ssh_config,
        r#"
Host app-a app-b web-*
#_Desc Shared app hosts
#_Profile prod
#_pass shared_key
HostName app.internal
User deploy
Port 2222
IdentityFile ~/.ssh/id_app
IdentitiesOnly yes
ProxyJump bastion
ProxyCommand ssh -W %h:%p bastion
ForwardAgent yes
LocalForward 8080 localhost:80
RemoteForward 9090 localhost:90

Host desktop01
#_Protocol rdp
#_RdpDomain ACME
#_RdpArgs /f +clipboard
#_hidden true
#_pass office_rdp
HostName rdp.internal
User administrator
Port 3390

Match host bastion
    User ignored
"#,
    )
    .expect("write ssh config");
    write_file(&inventory_path, "inventory: []\n").expect("write existing inventory");

    let summary = migrate_ssh_config_to_inventory(&ssh_config, &inventory_path).expect("migrate inventory");
    assert_eq!(summary.hosts_written, 3);
    assert_eq!(summary.wildcard_aliases_skipped, 1);
    assert_eq!(summary.unsupported_blocks_skipped, 1);
    assert!(summary.backup_path.is_some());

    let rendered = fs::read_to_string(&inventory_path).expect("read migrated inventory");
    assert!(rendered.contains("\n\n  - name: 'app-b'"));
    assert!(rendered.contains("vault_pass: 'shared_key'"));
    assert!(rendered.contains("identity_file:"));
    assert!(rendered.contains("identities_only: true"));
    assert!(rendered.contains("proxy_jump: 'bastion'"));
    assert!(rendered.contains("proxy_command: 'ssh -W %h:%p bastion'"));
    assert!(rendered.contains("forward_agent: 'yes'"));
    assert!(rendered.contains("local_forward:\n      - '8080:localhost:80'"));
    assert!(rendered.contains("remote_forward:\n      - '9090:localhost:90'"));
    assert!(rendered.contains("rdp_domain: 'ACME'"));
    assert!(rendered.contains("hidden: true"));

    let tree = build_inventory_tree(&inventory_path).expect("load migrated inventory");
    let app_a = tree.hosts.iter().find(|host| host.name == "app-a").expect("app-a");
    assert_eq!(app_a.protocol, ConnectionProtocol::Ssh);
    assert_eq!(app_a.host, "app.internal");
    assert_eq!(app_a.user.as_deref(), Some("deploy"));
    assert_eq!(app_a.port, Some(2222));
    assert_eq!(app_a.profile.as_deref(), Some("prod"));
    assert_eq!(app_a.vault_pass.as_deref(), Some("shared_key"));
    assert_eq!(app_a.ssh.local_forward, vec!["8080:localhost:80".to_string()]);
    assert_eq!(app_a.ssh.remote_forward, vec!["9090:localhost:90".to_string()]);

    let desktop = tree.hosts.iter().find(|host| host.name == "desktop01").expect("desktop01");
    assert_eq!(desktop.protocol, ConnectionProtocol::Rdp);
    assert_eq!(desktop.host, "rdp.internal");
    assert_eq!(desktop.rdp.domain.as_deref(), Some("ACME"));
    assert_eq!(desktop.rdp.args, vec!["/f".to_string(), "+clipboard".to_string()]);
    assert!(desktop.hidden);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn migrates_included_ssh_configs_into_folder_nodes() {
    let dir = test_dir("include_folders").expect("temp dir");
    let ssh_config = dir.join("config");
    let inventory_path = dir.join("cossh-inventory.yaml");

    write_file(
        &ssh_config,
        r#"
Host root-host
HostName root.internal

Include ./conf.d/*.conf
"#,
    )
    .expect("write root ssh config");
    write_file(
        &dir.join("conf.d/10-network.conf"),
        r#"
Host switch-a
#_Desc Network switch
HostName 10.0.0.10
"#,
    )
    .expect("write included ssh config");

    let summary = migrate_ssh_config_to_inventory(&ssh_config, &inventory_path).expect("migrate inventory");
    assert_eq!(summary.hosts_written, 2);
    assert_eq!(summary.wildcard_aliases_skipped, 0);

    let rendered = fs::read_to_string(&inventory_path).expect("read migrated inventory");
    assert!(rendered.contains("- '10-network':"));
    assert!(rendered.contains("\n      - name: 'switch-a'"));

    let tree = build_inventory_tree(&inventory_path).expect("load migrated inventory");
    assert_eq!(tree.root.host_indices.len(), 1);
    assert_eq!(tree.root.children.len(), 1);
    assert_eq!(tree.root.children[0].name, "10-network");

    let switch = tree.hosts.iter().find(|host| host.name == "switch-a").expect("switch-a");
    assert_eq!(switch.host, "10.0.0.10");
    assert_eq!(switch.description.as_deref(), Some("Network switch"));
    assert_eq!(switch.source_folder_path, vec!["10-network".to_string()]);

    let _ = fs::remove_dir_all(dir);
}
