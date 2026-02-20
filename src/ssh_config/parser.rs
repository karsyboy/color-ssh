//! SSH config file parser and include tree builder.

use super::include::{expand_include_pattern, resolve_include_pattern};
use super::model::{FolderId, SshHost, SshHostTreeModel, TreeFolder};
use super::path::expand_tilde;
use crate::log_debug;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ParsedConfigFile {
    hosts: Vec<SshHost>,
    include_patterns: Vec<String>,
}

/// Parse an SSH config file and return a list of hosts.
pub fn parse_ssh_config(config_path: &Path) -> io::Result<Vec<SshHost>> {
    Ok(build_ssh_host_tree(config_path)?.hosts)
}

pub(super) fn build_ssh_host_tree(config_path: &Path) -> io::Result<SshHostTreeModel> {
    let mut hosts = Vec::new();
    let mut visited = HashSet::new();
    let mut next_id: FolderId = 0;
    let root_name = config_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("config").to_string();

    let mut root = parse_tree_folder(config_path, &root_name, &mut hosts, &mut visited, &mut next_id)?.unwrap_or_else(|| TreeFolder {
        id: 0,
        name: root_name,
        path: config_path.to_path_buf(),
        children: Vec::new(),
        host_indices: Vec::new(),
    });
    sort_tree_folder(&mut root, &hosts);

    Ok(SshHostTreeModel { root, hosts })
}

fn sort_tree_folder(folder: &mut TreeFolder, hosts: &[SshHost]) {
    folder.host_indices.sort_by(|left_idx, right_idx| {
        let left_name = hosts.get(*left_idx).map(|host| host.name.as_str()).unwrap_or_default();
        let right_name = hosts.get(*right_idx).map(|host| host.name.as_str()).unwrap_or_default();
        let left_key = left_name.to_lowercase();
        let right_key = right_name.to_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left_name.cmp(right_name))
            .then_with(|| left_idx.cmp(right_idx))
    });

    for child in &mut folder.children {
        sort_tree_folder(child, hosts);
    }

    folder.children.sort_by(|left, right| {
        let left_key = left.name.to_lowercase();
        let right_key = right.name.to_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn parse_tree_folder(
    config_path: &Path,
    name: &str,
    hosts: &mut Vec<SshHost>,
    visited: &mut HashSet<PathBuf>,
    next_id: &mut FolderId,
) -> io::Result<Option<TreeFolder>> {
    let canonical = config_path.canonicalize().unwrap_or_else(|_| config_path.to_path_buf());

    if !visited.insert(canonical.clone()) {
        log_debug!("Skipping already visited SSH include file (possible include cycle): {}", canonical.display());
        return Ok(None);
    }

    let parsed = parse_config_file(&canonical)?;
    let folder_id = *next_id;
    *next_id += 1;

    let mut host_indices = Vec::new();
    for host in parsed.hosts {
        host_indices.push(hosts.len());
        hosts.push(host);
    }

    let mut children = Vec::new();
    let parent_dir = canonical.parent().unwrap_or(Path::new("."));

    for include_pattern in parsed.include_patterns {
        let resolved_pattern = resolve_include_pattern(&include_pattern, parent_dir);
        for include_path in expand_include_pattern(&resolved_pattern) {
            let child_name = include_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("include").to_string();

            if let Some(child) = parse_tree_folder(&include_path, &child_name, hosts, visited, next_id)? {
                children.push(child);
            }
        }
    }

    Ok(Some(TreeFolder {
        id: folder_id,
        name: name.to_string(),
        path: canonical,
        children,
        host_indices,
    }))
}

fn parse_config_file(config_path: &Path) -> io::Result<ParsedConfigFile> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);

    let mut parsed = ParsedConfigFile::default();
    let mut current_hosts: Vec<SshHost> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('#') {
            if let Some(desc) = trimmed.strip_prefix("#_Desc") {
                let desc = desc.trim().to_string();
                for host in &mut current_hosts {
                    host.description = Some(desc.clone());
                }
            }
            if let Some(profile) = trimmed.strip_prefix("#_Profile") {
                let profile = profile.trim().to_string();
                for host in &mut current_hosts {
                    host.profile = Some(profile.clone());
                }
            }
            if let Some(sshpass_val) = trimmed.strip_prefix("#_sshpass") {
                let val = sshpass_val.trim().to_lowercase();
                let use_sshpass = val == "true" || val == "yes" || val == "1";
                for host in &mut current_hosts {
                    host.use_sshpass = use_sshpass;
                }
            }
            if let Some(hidden_val) = trimmed.strip_prefix("#_hidden") {
                let val = hidden_val.trim().to_lowercase();
                let hidden = val == "true" || val == "yes" || val == "1";
                for host in &mut current_hosts {
                    host.hidden = hidden;
                }
            }
            continue;
        }

        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let keyword = parts[0].to_lowercase();
        let value = parts[1].trim();

        match keyword.as_str() {
            "host" => {
                for host in current_hosts.drain(..) {
                    if !host.name.contains('*') && !host.name.contains('?') && !host.hidden {
                        parsed.hosts.push(host);
                    }
                }

                current_hosts = value.split_whitespace().map(|alias| SshHost::new(alias.to_string())).collect();
                if current_hosts.is_empty() {
                    current_hosts.push(SshHost::new(value.to_string()));
                }
            }
            "hostname" => {
                for host in &mut current_hosts {
                    host.hostname = Some(value.to_string());
                }
            }
            "user" => {
                for host in &mut current_hosts {
                    host.user = Some(value.to_string());
                }
            }
            "port" => {
                if let Ok(port) = value.parse::<u16>() {
                    for host in &mut current_hosts {
                        host.port = Some(port);
                    }
                }
            }
            "identityfile" => {
                let identity = expand_tilde(value);
                for host in &mut current_hosts {
                    host.identity_file = Some(identity.clone());
                }
            }
            "proxyjump" => {
                for host in &mut current_hosts {
                    host.proxy_jump = Some(value.to_string());
                }
            }
            "localforward" => {
                for host in &mut current_hosts {
                    host.local_forward.push(value.to_string());
                }
            }
            "remoteforward" => {
                for host in &mut current_hosts {
                    host.remote_forward.push(value.to_string());
                }
            }
            "include" => {
                for token in value.split_whitespace() {
                    parsed.include_patterns.push(token.to_string());
                }
            }
            _ => {
                for host in &mut current_hosts {
                    host.other_options.insert(keyword.clone(), value.to_string());
                }
            }
        }
    }

    for host in current_hosts {
        if !host.name.contains('*') && !host.name.contains('?') && !host.hidden {
            parsed.hosts.push(host);
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
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
}
