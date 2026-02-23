use super::parse_ssh_config;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn test_dir(name: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cossh_ssh_config_{name}_{nanos}_{serial}"));
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
fn parses_metadata_and_filters_hidden_wildcard_hosts() {
    let dir = test_dir("metadata_filter").expect("temp dir");
    let config_path = dir.join("config");

    write_file(
            &config_path,
            "Host app\n#_Desc Production app\n#_Profile prod\n#_sshpass yes\nHostName 10.0.0.10\nUser deploy\nPort 2200\nIdentityFile ~/.ssh/id_app\nLocalForward 8080 localhost:80\nRemoteForward 9090 localhost:90\nCompression yes\n\nHost hidden-node\n#_hidden true\nHostName 10.0.0.20\n\nHost web-*\nHostName 10.0.0.30\n",
        )
        .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    assert_eq!(hosts.len(), 1);

    let host = &hosts[0];
    assert_eq!(host.name, "app");
    assert_eq!(host.hostname.as_deref(), Some("10.0.0.10"));
    assert_eq!(host.user.as_deref(), Some("deploy"));
    assert_eq!(host.port, Some(2200));
    assert_eq!(host.description.as_deref(), Some("Production app"));
    assert_eq!(host.profile.as_deref(), Some("prod"));
    assert!(host.use_sshpass);
    assert!(!host.hidden);
    assert_eq!(host.local_forward, vec!["8080 localhost:80"]);
    assert_eq!(host.remote_forward, vec!["9090 localhost:90"]);
    assert_eq!(host.other_options.get("compression").map(String::as_str), Some("yes"));

    let identity = host.identity_file.as_deref().unwrap_or_default();
    assert!(identity.ends_with(".ssh/id_app"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn expands_wildcard_includes_in_sorted_order() {
    let dir = test_dir("include_order").expect("temp dir");
    let config_path = dir.join("config");

    write_file(&config_path, "Host root\nHostName root.example\nInclude conf.d/*.conf\n").expect("write root config");

    write_file(&dir.join("conf.d/20-b.conf"), "Host b\nHostName b.example\n").expect("write b include");
    write_file(&dir.join("conf.d/10-a.conf"), "Host a\nHostName a.example\n").expect("write a include");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let names: Vec<&str> = hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["root", "a", "b"]);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn handles_include_cycles_without_recursing_forever() {
    let dir = test_dir("include_cycle").expect("temp dir");
    let config_path = dir.join("config");

    write_file(&config_path, "Host root\nHostName root.example\nInclude include/sub.conf\n").expect("write root config");

    write_file(&dir.join("include/sub.conf"), "Host sub\nHostName sub.example\nInclude ../config\n").expect("write sub config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let names: Vec<&str> = hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["root", "sub"]);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn expands_multi_alias_host_stanzas_and_applies_metadata_to_all_aliases() {
    let dir = test_dir("multi_alias").expect("temp dir");
    let config_path = dir.join("config");

    write_file(
            &config_path,
            "Host app-a app-b app-c\n#_Desc Shared app hosts\n#_Profile prod\nHostName app.internal\nUser deploy\nPort 2222\nIdentityFile ~/.ssh/id_app\nProxyJump bastion\n",
        )
        .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let names: Vec<&str> = hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["app-a", "app-b", "app-c"]);

    for host in hosts {
        assert_eq!(host.hostname.as_deref(), Some("app.internal"));
        assert_eq!(host.user.as_deref(), Some("deploy"));
        assert_eq!(host.port, Some(2222));
        assert_eq!(host.description.as_deref(), Some("Shared app hosts"));
        assert_eq!(host.profile.as_deref(), Some("prod"));
        assert_eq!(host.proxy_jump.as_deref(), Some("bastion"));
        let identity = host.identity_file.as_deref().unwrap_or_default();
        assert!(identity.ends_with(".ssh/id_app"));
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn filters_hidden_and_wildcard_aliases_within_multi_alias_stanza() {
    let dir = test_dir("alias_filtering").expect("temp dir");
    let config_path = dir.join("config");

    write_file(
        &config_path,
        "Host db-* db-primary db-standby\nHostName db.internal\n\nHost hidden-a hidden-b\n#_hidden true\nHostName hidden.internal\n",
    )
    .expect("write config");

    let hosts = parse_ssh_config(&config_path).expect("parse config");
    let names: Vec<&str> = hosts.iter().map(|host| host.name.as_str()).collect();
    assert_eq!(names, vec!["db-primary", "db-standby"]);

    let _ = fs::remove_dir_all(dir);
}
