//! SSH config file parser
//!
//! Parses SSH configuration files and extracts host information.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Represents a single SSH host configuration
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

/// Parse an SSH config file and return a list of hosts
pub fn parse_ssh_config(config_path: &Path) -> std::io::Result<Vec<SshHost>> {
    let file = File::open(config_path)?;
    let reader = BufReader::new(file);

    let mut hosts = Vec::new();
    let mut current_host: Option<SshHost> = None;
    let mut included_files = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split line into keyword and value
        let parts: Vec<&str> = trimmed.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let keyword = parts[0].to_lowercase();
        let value = parts[1].trim();

        match keyword.as_str() {
            "host" => {
                // Save previous host if exists
                if let Some(host) = current_host.take() {
                    // Skip wildcard hosts
                    if !host.name.contains('*') && !host.name.contains('?') {
                        hosts.push(host);
                    }
                }

                // Start new host (only take first host pattern)
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
                if let Some(ref mut host) = current_host {
                    if let Ok(port) = value.parse::<u16>() {
                        host.port = Some(port);
                    }
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
                // Track included files for processing
                let expanded_path = expand_tilde(value);
                included_files.push(expanded_path);
            }
            _ => {
                // Store other options
                if let Some(ref mut host) = current_host {
                    host.other_options.insert(keyword.clone(), value.to_string());
                }
            }
        }
    }

    // Don't forget the last host
    if let Some(host) = current_host {
        if !host.name.contains('*') && !host.name.contains('?') {
            hosts.push(host);
        }
    }

    // Process included files
    for include_path in included_files {
        if let Ok(included_hosts) = parse_ssh_config_with_glob(&include_path) {
            hosts.extend(included_hosts);
        }
    }

    Ok(hosts)
}

/// Parse SSH config with glob pattern support
fn parse_ssh_config_with_glob(pattern: &str) -> std::io::Result<Vec<SshHost>> {
    let path = PathBuf::from(pattern);

    // If the path doesn't contain wildcards, just parse it
    if !pattern.contains('*') && !pattern.contains('?') {
        if path.exists() {
            return parse_ssh_config(&path);
        } else {
            return Ok(Vec::new());
        }
    }

    // Handle glob patterns (simple implementation)
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename_pattern = path.file_name().and_then(|s| s.to_str()).unwrap_or("*");

    let mut all_hosts = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(file_name) = entry.file_name().into_string() {
                if matches_pattern(&file_name, filename_pattern) {
                    if let Ok(hosts) = parse_ssh_config(&entry.path()) {
                        all_hosts.extend(hosts);
                    }
                }
            }
        }
    }

    Ok(all_hosts)
}

/// Simple pattern matching for filenames (supports * and ? wildcards)
fn matches_pattern(text: &str, pattern: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut pattern_idx = 0;
    let mut text_idx = 0;

    while pattern_idx < pattern_chars.len() && text_idx < text_chars.len() {
        match pattern_chars[pattern_idx] {
            '*' => {
                // Try to match zero or more characters
                if pattern_idx == pattern_chars.len() - 1 {
                    return true; // * at end matches everything
                }
                pattern_idx += 1;
                // Try to find next pattern character
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
                // Matches any single character
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

    // Check if we've consumed both strings
    pattern_idx == pattern_chars.len() && text_idx == text_chars.len()
}

/// Expand ~ to home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

/// Get the default SSH config path
pub fn get_default_ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ssh").join("config"))
}

/// Load all SSH hosts from the default config
pub fn load_ssh_hosts() -> std::io::Result<Vec<SshHost>> {
    let config_path = get_default_ssh_config_path().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not find home directory"))?;

    if !config_path.exists() {
        return Ok(Vec::new());
    }

    parse_ssh_config(&config_path)
}
