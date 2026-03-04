//! SSH config domain models.

use crate::inventory::{ConnectionProtocol, SshOptionMap, TreeFolder};
use std::path::PathBuf;

/// Represents a single SSH host configuration.
#[derive(Debug, Clone)]
pub struct SshHost {
    /// The host name/alias from the config.
    pub name: String,
    /// Connection protocol for this host.
    pub protocol: ConnectionProtocol,
    /// Hostname (or IP address).
    pub hostname: Option<String>,
    /// Username.
    pub user: Option<String>,
    /// Port number.
    pub port: Option<u16>,
    /// Identity file path.
    pub identity_files: Vec<String>,
    /// Whether only explicit identities should be used.
    pub identities_only: Option<bool>,
    /// Proxy jump host.
    pub proxy_jump: Option<String>,
    /// Proxy command setting.
    pub proxy_command: Option<String>,
    /// ForwardAgent value.
    pub forward_agent: Option<String>,
    /// Description from `#_Desc` comment.
    pub description: Option<String>,
    /// Profile from `#_Profile` comment.
    pub profile: Option<String>,
    /// Password key name (from `#_pass` comment).
    pub pass_key: Option<String>,
    /// Optional RDP domain (from `#_RdpDomain` comment).
    pub rdp_domain: Option<String>,
    /// Additional FreeRDP client arguments (from `#_RdpArgs` comments).
    pub rdp_args: Vec<String>,
    /// Whether to hide this host from the interactive host view (from `#_hidden` comment).
    pub hidden: bool,
    /// Local forward settings.
    pub local_forward: Vec<String>,
    /// Remote forward settings.
    pub remote_forward: Vec<String>,
    /// Additional custom options.
    pub other_options: SshOptionMap,
}

impl SshHost {
    // Construction.
    /// Create a new `SshHost` with just a name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            protocol: ConnectionProtocol::Ssh,
            hostname: None,
            user: None,
            port: None,
            identity_files: Vec::new(),
            identities_only: None,
            proxy_jump: None,
            proxy_command: None,
            forward_agent: None,
            description: None,
            profile: None,
            pass_key: None,
            rdp_domain: None,
            rdp_args: Vec::new(),
            hidden: false,
            local_forward: Vec::new(),
            remote_forward: Vec::new(),
            other_options: SshOptionMap::new(),
        }
    }
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
    #[allow(dead_code)]
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
