//! SSH config domain models.

use std::collections::HashMap;
use std::path::PathBuf;

/// Stable folder identifier used by the TUI tree.
pub type FolderId = usize;

/// Represents a single SSH host configuration.
#[derive(Debug, Clone)]
pub struct SshHost {
    /// The host name/alias from the config.
    pub name: String,
    /// Hostname (or IP address).
    pub hostname: Option<String>,
    /// Username.
    pub user: Option<String>,
    /// Port number.
    pub port: Option<u16>,
    /// Identity file path.
    pub identity_file: Option<String>,
    /// Proxy jump host.
    pub proxy_jump: Option<String>,
    /// Description from `#_Desc` comment.
    pub description: Option<String>,
    /// Profile from `#_Profile` comment.
    pub profile: Option<String>,
    /// Whether to use sshpass (from `#_sshpass` comment).
    pub use_sshpass: bool,
    /// Whether to hide this host from the interactive host view (from `#_hidden` comment).
    pub hidden: bool,
    /// Local forward settings.
    pub local_forward: Vec<String>,
    /// Remote forward settings.
    pub remote_forward: Vec<String>,
    /// Additional custom options.
    pub other_options: HashMap<String, String>,
}

impl SshHost {
    // Construction.
    /// Create a new `SshHost` with just a name.
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
}

/// Tree folder node derived from include relationships.
#[derive(Debug, Clone)]
pub struct TreeFolder {
    /// Stable folder ID.
    pub id: FolderId,
    /// Display name (file basename).
    pub name: String,
    /// Source config file path.
    pub path: PathBuf,
    /// Included child folders.
    pub children: Vec<TreeFolder>,
    /// Host indices (into [`SshHostTreeModel::hosts`]) defined in this file.
    pub host_indices: Vec<usize>,
}

/// Parsed SSH host data and include graph as a folder tree.
#[derive(Debug, Clone)]
pub struct SshHostTreeModel {
    /// Root folder (`~/.ssh/config`).
    pub root: TreeFolder,
    /// Flattened host list in discovery order.
    pub hosts: Vec<SshHost>,
}

impl SshHostTreeModel {
    // Construction helpers.
    pub(super) fn empty(root_path: PathBuf) -> Self {
        let root_name = root_path.file_name().and_then(|segment| segment.to_str()).unwrap_or("config").to_string();
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
