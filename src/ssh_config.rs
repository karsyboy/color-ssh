//! SSH config file parser
//!
//! Parses SSH configuration files and extracts host information.

use crate::log_debug;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Stable folder identifier used by the TUI tree.
pub type FolderId = usize;

/// Represents a single SSH host configuration.
#[derive(Debug, Clone)]
pub struct SshHost {
    /// The host name/alias from the config
    pub name: String,
    /// Hostname (or IP address)
    pub hostname: Option<String>,
    /// Username
    pub user: Option<String>,
    /// Port number
    pub port: Option<u16>,
    /// Identity file path
    pub identity_file: Option<String>,
    /// Proxy jump host
    pub proxy_jump: Option<String>,
    /// Description from #_Desc comment
    pub description: Option<String>,
    /// Profile from #_Profile comment (cossh config profile to use)
    pub profile: Option<String>,
    /// Whether to use sshpass (from #_sshpass comment)
    pub use_sshpass: bool,
    /// Whether to hide this host from the interactive host view (from #_hidden comment)
    pub hidden: bool,
    /// Local forward settings
    pub local_forward: Vec<String>,
    /// Remote forward settings
    pub remote_forward: Vec<String>,
    /// Additional custom options
    pub other_options: HashMap<String, String>,
}

impl SshHost {
    /// Create a new SshHost with just a name
    pub fn new(name: String) -> Self {
        Self {
            name,
            hostname: None,
            user: None,
            port: None,
            identity_file: None,
            proxy_jump: None,
            description: None,
            profile: None,
            use_sshpass: false,
            hidden: false,
            local_forward: Vec::new(),
            remote_forward: Vec::new(),
            other_options: HashMap::new(),
        }
    }

    /// Get a formatted display string for the host details
    pub fn get_details(&self) -> String {
        let mut details = Vec::new();

        details.push(format!("Host: {}", self.name));

        if let Some(hostname) = &self.hostname {
            details.push(format!("  Hostname: {}", hostname));
        }

        if let Some(user) = &self.user {
            details.push(format!("  User: {}", user));
        }

        if let Some(port) = &self.port {
            details.push(format!("  Port: {}", port));
        }

        if let Some(identity) = &self.identity_file {
            details.push(format!("  IdentityFile: {}", identity));
        }

        if let Some(proxy) = &self.proxy_jump {
            details.push(format!("  ProxyJump: {}", proxy));
        }

        if let Some(desc) = &self.description {
            details.push(format!("  Description: {}", desc));
        }

        if let Some(profile) = &self.profile {
            details.push(format!("  Profile: {}", profile));
        }

        if self.use_sshpass {
            details.push("  SSHPass: enabled".to_string());
        }

        if !self.local_forward.is_empty() {
            for fwd in &self.local_forward {
                details.push(format!("  LocalForward: {}", fwd));
            }
        }

        if !self.remote_forward.is_empty() {
            for fwd in &self.remote_forward {
                details.push(format!("  RemoteForward: {}", fwd));
            }
        }

        for (key, value) in &self.other_options {
            details.push(format!("  {}: {}", key, value));
        }

        details.join("\n")
    }
}

/// Tree folder node derived from include relationships.
#[derive(Debug, Clone)]
pub struct TreeFolder {
    /// Stable folder ID
    pub id: FolderId,
    /// Display name (file basename)
    pub name: String,
    /// Source config file path
    pub path: PathBuf,
    /// Included child folders
    pub children: Vec<TreeFolder>,
    /// Host indices (into [`SshHostTreeModel::hosts`]) defined in this file
    pub host_indices: Vec<usize>,
}

/// Parsed SSH host data and include graph as a folder tree.
#[derive(Debug, Clone)]
pub struct SshHostTreeModel {
    /// Root folder (`~/.ssh/config`)
    pub root: TreeFolder,
    /// Flattened host list in discovery order
    pub hosts: Vec<SshHost>,
}

impl SshHostTreeModel {
    fn empty(root_path: PathBuf) -> Self {
        let root_name = root_path.file_name().and_then(|s| s.to_str()).unwrap_or("config").to_string();
        Self {
            root: TreeFolder {
                id: 0,
                name: root_name,
                path: root_path,
                children: Vec::new(),
                host_indices: Vec::new(),
            },
            hosts: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct ParsedConfigFile {
    hosts: Vec<SshHost>,
    include_patterns: Vec<String>,
}

/// Parse an SSH config file and return a list of hosts.
pub fn parse_ssh_config(config_path: &Path) -> std::io::Result<Vec<SshHost>> {
    Ok(build_ssh_host_tree(config_path)?.hosts)
}

fn build_ssh_host_tree(config_path: &Path) -> std::io::Result<SshHostTreeModel> {
    let mut hosts = Vec::new();
    let mut visited = HashSet::new();
    let mut next_id: FolderId = 0;
    let root_name = config_path.file_name().and_then(|s| s.to_str()).unwrap_or("config").to_string();

    let root = parse_tree_folder(config_path, &root_name, &mut hosts, &mut visited, &mut next_id)?.unwrap_or_else(|| TreeFolder {
        id: 0,
        name: root_name,
        path: config_path.to_path_buf(),
        children: Vec::new(),
        host_indices: Vec::new(),
    });

    Ok(SshHostTreeModel { root, hosts })
}

fn parse_tree_folder(
    config_path: &Path,
    name: &str,
    hosts: &mut Vec<SshHost>,
    visited: &mut HashSet<PathBuf>,
    next_id: &mut FolderId,
) -> std::io::Result<Option<TreeFolder>> {
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
            let child_name = include_path.file_name().and_then(|s| s.to_str()).unwrap_or("include").to_string();

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

fn parse_config_file(config_path: &Path) -> std::io::Result<ParsedConfigFile> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);

    let mut parsed = ParsedConfigFile::default();
    let mut current_host: Option<SshHost> = None;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Parse #_ comments for host metadata
        if trimmed.starts_with('#') {
            if let Some(desc) = trimmed.strip_prefix("#_Desc")
                && let Some(ref mut host) = current_host
            {
                host.description = Some(desc.trim().to_string());
            }
            if let Some(profile) = trimmed.strip_prefix("#_Profile")
                && let Some(ref mut host) = current_host
            {
                host.profile = Some(profile.trim().to_string());
            }
            if let Some(sshpass_val) = trimmed.strip_prefix("#_sshpass")
                && let Some(ref mut host) = current_host
            {
                let val = sshpass_val.trim().to_lowercase();
                host.use_sshpass = val == "true" || val == "yes" || val == "1";
            }
            if let Some(hidden_val) = trimmed.strip_prefix("#_hidden")
                && let Some(ref mut host) = current_host
            {
                let val = hidden_val.trim().to_lowercase();
                host.hidden = val == "true" || val == "yes" || val == "1";
            }
            continue;
        }

        // Split line into keyword and value.
        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let keyword = parts[0].to_lowercase();
        let value = parts[1].trim();

        match keyword.as_str() {
            "host" => {
                // Save previous host if exists.
                if let Some(host) = current_host.take()
                    && !host.name.contains('*')
                    && !host.name.contains('?')
                    && !host.hidden
                {
                    parsed.hosts.push(host);
                }

                // Start new host (only take first host pattern).
                let host_pattern = value.split_whitespace().next().unwrap_or(value);
                current_host = Some(SshHost::new(host_pattern.to_string()));
            }
            "hostname" => {
                if let Some(ref mut host) = current_host {
                    host.hostname = Some(value.to_string());
                }
            }
            "user" => {
                if let Some(ref mut host) = current_host {
                    host.user = Some(value.to_string());
                }
            }
            "port" => {
                if let Some(ref mut host) = current_host
                    && let Ok(port) = value.parse::<u16>()
                {
                    host.port = Some(port);
                }
            }
            "identityfile" => {
                if let Some(ref mut host) = current_host {
                    host.identity_file = Some(expand_tilde(value));
                }
            }
            "proxyjump" => {
                if let Some(ref mut host) = current_host {
                    host.proxy_jump = Some(value.to_string());
                }
            }
            "localforward" => {
                if let Some(ref mut host) = current_host {
                    host.local_forward.push(value.to_string());
                }
            }
            "remoteforward" => {
                if let Some(ref mut host) = current_host {
                    host.remote_forward.push(value.to_string());
                }
            }
            "include" => {
                for token in value.split_whitespace() {
                    parsed.include_patterns.push(token.to_string());
                }
            }
            _ => {
                // Store other options.
                if let Some(ref mut host) = current_host {
                    host.other_options.insert(keyword.clone(), value.to_string());
                }
            }
        }
    }

    // Don't forget the last host.
    if let Some(host) = current_host
        && !host.name.contains('*')
        && !host.name.contains('?')
        && !host.hidden
    {
        parsed.hosts.push(host);
    }

    Ok(parsed)
}

fn resolve_include_pattern(pattern: &str, base_dir: &Path) -> String {
    let expanded = expand_tilde(pattern);
    let expanded_path = PathBuf::from(&expanded);
    if expanded_path.is_absolute() {
        expanded
    } else {
        base_dir.join(expanded_path).to_string_lossy().to_string()
    }
}

fn expand_include_pattern(pattern: &str) -> Vec<PathBuf> {
    let path = PathBuf::from(pattern);

    // If the path doesn't contain wildcards, use it as-is.
    if !pattern.contains('*') && !pattern.contains('?') {
        if path.is_file() {
            return vec![path];
        }
        return Vec::new();
    }

    // Handle glob-style patterns with simple wildcard support.
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename_pattern = path.file_name().and_then(|s| s.to_str()).unwrap_or("*");

    let mut matched_paths: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            if let Ok(file_name) = entry.file_name().into_string()
                && matches_pattern(&file_name, filename_pattern)
            {
                matched_paths.push(entry.path());
            }
        }
    }

    matched_paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    matched_paths
}

/// Simple pattern matching for filenames (supports * and ? wildcards).
fn matches_pattern(text: &str, pattern: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut pattern_idx = 0;
    let mut text_idx = 0;

    while pattern_idx < pattern_chars.len() && text_idx < text_chars.len() {
        match pattern_chars[pattern_idx] {
            '*' => {
                // Try to match zero or more characters.
                if pattern_idx == pattern_chars.len() - 1 {
                    return true; // * at end matches everything
                }
                pattern_idx += 1;
                // Try to find next pattern character.
                while text_idx < text_chars.len() {
                    if matches_pattern(
                        &text_chars[text_idx..].iter().collect::<String>(),
                        &pattern_chars[pattern_idx..].iter().collect::<String>(),
                    ) {
                        return true;
                    }
                    text_idx += 1;
                }
                return false;
            }
            '?' => {
                // Matches any single character.
                text_idx += 1;
                pattern_idx += 1;
            }
            c => {
                if text_chars[text_idx] != c {
                    return false;
                }
                text_idx += 1;
                pattern_idx += 1;
            }
        }
    }

    // Check if we've consumed both strings.
    pattern_idx == pattern_chars.len() && text_idx == text_chars.len()
}

/// Expand ~ to home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

/// Get the default SSH config path.
pub fn get_default_ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

/// Load the SSH include tree rooted at `~/.ssh/config`.
pub(crate) fn load_ssh_host_tree() -> std::io::Result<SshHostTreeModel> {
    let config_path = get_default_ssh_config_path().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not find home directory"))?;

    if !config_path.exists() {
        return Ok(SshHostTreeModel::empty(config_path));
    }

    build_ssh_host_tree(&config_path)
}

/// Load all SSH hosts from the default config.
pub fn load_ssh_hosts() -> std::io::Result<Vec<SshHost>> {
    Ok(load_ssh_host_tree()?.hosts)
}
