//! SSH config file parser and include tree builder.

use super::include::{expand_include_pattern, resolve_include_pattern};
use super::model::{SshHost, SshHostTreeModel};
use super::path::expand_tilde;
use crate::inventory::{ConnectionProtocol, FolderId, TreeFolder, sort_tree_folder_by_host_name};
use crate::log_debug;
use crate::validation::validate_vault_entry_name;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ParsedConfigFile {
    hosts: Vec<SshHost>,
    include_patterns: Vec<String>,
    unsupported_blocks: usize,
}

#[derive(Debug, Clone, Copy)]
struct ParseOptions {
    filter_runtime_only_hosts: bool,
}

#[derive(Debug, Clone)]
pub struct MigrationParseResult {
    pub root: TreeFolder,
    pub hosts: Vec<SshHost>,
    pub unsupported_blocks: usize,
}

impl ParseOptions {
    const RUNTIME: Self = Self {
        filter_runtime_only_hosts: true,
    };

    const MIGRATION: Self = Self {
        filter_runtime_only_hosts: false,
    };
}

fn parse_protocol_tag(value: &str) -> ConnectionProtocol {
    ConnectionProtocol::from(value)
}

/// Parse an SSH config file and return a list of visible hosts.
pub fn parse_ssh_config(config_path: &Path) -> io::Result<Vec<SshHost>> {
    Ok(build_ssh_host_tree(config_path)?.hosts)
}

/// Parse an SSH config file for YAML inventory migration.
pub fn parse_ssh_config_for_migration(config_path: &Path) -> io::Result<MigrationParseResult> {
    let mut hosts = Vec::new();
    let mut visited = HashSet::new();
    let mut next_id: FolderId = 0;
    let mut unsupported_blocks = 0usize;
    let root_name = config_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("config").to_string();

    let root = parse_tree_folder(
        config_path,
        &root_name,
        &mut hosts,
        &mut visited,
        &mut next_id,
        ParseOptions::MIGRATION,
        &mut unsupported_blocks,
    )?
    .unwrap_or_else(|| TreeFolder {
        id: 0,
        name: root_name,
        path: config_path.to_path_buf(),
        children: Vec::new(),
        host_indices: Vec::new(),
    });

    Ok(MigrationParseResult {
        root,
        hosts,
        unsupported_blocks,
    })
}

pub(super) fn build_ssh_host_tree(config_path: &Path) -> io::Result<SshHostTreeModel> {
    let mut hosts = Vec::new();
    let mut visited = HashSet::new();
    let mut next_id: FolderId = 0;
    let root_name = config_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("config").to_string();

    let mut unsupported_blocks = 0usize;
    let mut root = parse_tree_folder(
        config_path,
        &root_name,
        &mut hosts,
        &mut visited,
        &mut next_id,
        ParseOptions::RUNTIME,
        &mut unsupported_blocks,
    )?
    .unwrap_or_else(|| TreeFolder {
        id: 0,
        name: root_name,
        path: config_path.to_path_buf(),
        children: Vec::new(),
        host_indices: Vec::new(),
    });
    sort_tree_folder_by_host_name(&mut root, &hosts, |host| host.name.as_str());

    Ok(SshHostTreeModel { root, hosts })
}

fn parse_tree_folder(
    config_path: &Path,
    name: &str,
    hosts: &mut Vec<SshHost>,
    visited: &mut HashSet<PathBuf>,
    next_id: &mut FolderId,
    options: ParseOptions,
    unsupported_blocks: &mut usize,
) -> io::Result<Option<TreeFolder>> {
    let canonical = config_path.canonicalize().unwrap_or_else(|_| config_path.to_path_buf());

    if !visited.insert(canonical.clone()) {
        log_debug!("Skipping already visited SSH include file (possible include cycle): {}", canonical.display());
        return Ok(None);
    }

    let parsed = parse_config_file(&canonical, options)?;
    *unsupported_blocks += parsed.unsupported_blocks;
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

            if let Some(child) = parse_tree_folder(&include_path, &child_name, hosts, visited, next_id, options, unsupported_blocks)? {
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

fn finalize_current_hosts(parsed: &mut ParsedConfigFile, current_hosts: &mut Vec<SshHost>, options: ParseOptions) {
    for host in current_hosts.drain(..) {
        if options.filter_runtime_only_hosts && (host.name.contains('*') || host.name.contains('?') || host.hidden) {
            continue;
        }
        parsed.hosts.push(host);
    }
}

fn parse_bool_like(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn normalize_yes_no_string(value: &str) -> String {
    match parse_bool_like(value) {
        Some(true) => "yes".to_string(),
        Some(false) => "no".to_string(),
        None => value.trim().to_string(),
    }
}

fn push_other_option(host: &mut SshHost, key: &str, value: &str) {
    host.other_options.entry(key.to_string()).or_default().push(value.to_string());
}

fn parse_config_file(config_path: &Path, options: ParseOptions) -> io::Result<ParsedConfigFile> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);

    let mut parsed = ParsedConfigFile::default();
    let mut current_hosts: Vec<SshHost> = Vec::new();
    let mut in_match_block = false;

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
            if let Some(protocol) = trimmed.strip_prefix("#_Protocol") {
                let protocol = parse_protocol_tag(protocol.trim());
                for host in &mut current_hosts {
                    host.protocol = protocol.clone();
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
                if validate_vault_entry_name(pass_key) {
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
            if let Some(domain) = trimmed.strip_prefix("#_RdpDomain") {
                let domain = domain.trim();
                let domain = (!domain.is_empty()).then(|| domain.to_string());
                for host in &mut current_hosts {
                    host.rdp_domain = domain.clone();
                }
            }
            if let Some(args) = trimmed.strip_prefix("#_RdpArgs") {
                let args: Vec<String> = args.split_whitespace().map(str::to_string).collect();
                if !args.is_empty() {
                    for host in &mut current_hosts {
                        host.rdp_args.extend(args.iter().cloned());
                    }
                }
            }
            if let Some(hidden_val) = trimmed.strip_prefix("#_hidden") {
                let hidden = parse_bool_like(hidden_val).unwrap_or(false);
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

        let keyword = parts[0].to_ascii_lowercase();
        let value = parts[1].trim();

        if in_match_block && keyword != "host" && keyword != "match" {
            continue;
        }

        match keyword.as_str() {
            "host" => {
                in_match_block = false;
                finalize_current_hosts(&mut parsed, &mut current_hosts, options);

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
                    host.identity_files.push(identity.clone());
                }
            }
            "identitiesonly" => {
                let parsed_bool = parse_bool_like(value);
                for host in &mut current_hosts {
                    host.identities_only = parsed_bool;
                }
            }
            "proxyjump" => {
                for host in &mut current_hosts {
                    host.proxy_jump = Some(value.to_string());
                }
            }
            "proxycommand" => {
                for host in &mut current_hosts {
                    host.proxy_command = Some(value.to_string());
                }
            }
            "forwardagent" => {
                for host in &mut current_hosts {
                    host.forward_agent = Some(normalize_yes_no_string(value));
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
            "match" => {
                finalize_current_hosts(&mut parsed, &mut current_hosts, options);
                parsed.unsupported_blocks += 1;
                in_match_block = true;
            }
            _ => {
                for host in &mut current_hosts {
                    push_other_option(host, &keyword, value);
                }
            }
        }
    }

    finalize_current_hosts(&mut parsed, &mut current_hosts, options);
    Ok(parsed)
}

#[cfg(test)]
#[path = "../test/ssh_config/parser.rs"]
mod tests;
