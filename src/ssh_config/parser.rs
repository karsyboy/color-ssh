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

fn is_valid_pass_key_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
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
            if let Some(pass_val) = trimmed.strip_prefix("#_pass") {
                let pass_key = pass_val.trim();
                if is_valid_pass_key_name(pass_key) {
                    for host in &mut current_hosts {
                        host.pass_key = Some(pass_key.to_string());
                    }
                } else {
                    log_debug!("Ignoring invalid #_pass key name: {:?}", pass_key);
                    for host in &mut current_hosts {
                        host.pass_key = None;
                    }
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
#[path = "../test/ssh_config/parser.rs"]
mod tests;
