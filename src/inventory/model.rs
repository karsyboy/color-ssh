//! Inventory domain models.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// Stable folder identifier used by the TUI tree.
pub type FolderId = usize;

/// Arbitrary SSH option map (`option_name -> values`).
pub type SshOptionMap = BTreeMap<String, Vec<String>>;

/// Inventory-level connection protocol.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionProtocol {
    /// OpenSSH launch.
    #[default]
    Ssh,
    /// FreeRDP launch.
    Rdp,
    /// Preserved unknown protocol string.
    Other(String),
}

impl ConnectionProtocol {
    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "ssh" => Self::Ssh,
            "rdp" => Self::Rdp,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Ssh => "ssh",
            Self::Rdp => "rdp",
            Self::Other(value) => value.as_str(),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Ssh => "SSH",
            Self::Rdp => "RDP",
            Self::Other(value) => value.as_str(),
        }
    }
}

impl From<&str> for ConnectionProtocol {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

impl FromStr for ConnectionProtocol {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

impl fmt::Display for ConnectionProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SSH-specific inventory options.
#[derive(Debug, Clone, Default)]
pub struct SshHostOptions {
    /// One or more identity files.
    pub identity_files: Vec<String>,
    /// Equivalent to SSH `IdentitiesOnly`.
    pub identities_only: Option<bool>,
    /// SSH `ProxyJump`.
    pub proxy_jump: Option<String>,
    /// SSH `ProxyCommand`.
    pub proxy_command: Option<String>,
    /// SSH `ForwardAgent`.
    pub forward_agent: Option<String>,
    /// SSH `LocalForward` entries.
    pub local_forward: Vec<String>,
    /// SSH `RemoteForward` entries.
    pub remote_forward: Vec<String>,
    /// Additional SSH options not promoted to first-class fields.
    pub extra_options: SshOptionMap,
}

/// RDP-specific inventory options.
#[derive(Debug, Clone, Default)]
pub struct RdpHostOptions {
    /// Optional RDP domain.
    pub domain: Option<String>,
    /// Additional FreeRDP args.
    pub args: Vec<String>,
}

/// Fully normalized host record loaded from inventory files.
#[derive(Debug, Clone)]
pub struct InventoryHost {
    /// User-facing alias.
    pub name: String,
    /// Optional description shown in UI.
    pub description: Option<String>,
    /// Launch protocol.
    pub protocol: ConnectionProtocol,
    /// Destination hostname or IP.
    pub host: String,
    /// Optional login user.
    pub user: Option<String>,
    /// Optional destination port.
    pub port: Option<u16>,
    /// Optional runtime profile.
    pub profile: Option<String>,
    /// Optional vault entry name.
    pub vault_pass: Option<String>,
    /// Whether to hide this host from runtime host lists.
    pub hidden: bool,
    /// SSH-specific options.
    pub ssh: SshHostOptions,
    /// RDP-specific options.
    pub rdp: RdpHostOptions,
    /// Source inventory file where this host was loaded.
    pub source_file: PathBuf,
    /// Folder path from root to this host.
    pub source_folder_path: Vec<String>,
}

impl InventoryHost {
    /// Construct a host with SSH defaults using `name` as alias and host.
    pub fn new(name: String) -> Self {
        Self {
            host: name.clone(),
            name,
            description: None,
            protocol: ConnectionProtocol::Ssh,
            user: None,
            port: None,
            profile: None,
            vault_pass: None,
            hidden: false,
            ssh: SshHostOptions::default(),
            rdp: RdpHostOptions::default(),
            source_file: PathBuf::new(),
            source_folder_path: Vec::new(),
        }
    }
}

/// Tree folder node used by the TUI tree.
#[derive(Debug, Clone)]
pub struct TreeFolder {
    /// Stable folder id.
    pub id: FolderId,
    /// Folder display name.
    pub name: String,
    /// Source file path represented by this folder.
    pub path: PathBuf,
    /// Nested folders.
    pub children: Vec<TreeFolder>,
    /// Indices into [`InventoryTreeModel::hosts`].
    pub host_indices: Vec<usize>,
}

/// Parsed inventory data and folder tree.
#[derive(Debug, Clone)]
pub struct InventoryTreeModel {
    /// Folder tree rooted at the main inventory file.
    pub root: TreeFolder,
    /// Flattened hosts in tree traversal order.
    pub hosts: Vec<InventoryHost>,
}

impl InventoryTreeModel {
    pub(super) fn empty(root_path: PathBuf) -> Self {
        let root_name = root_path
            .file_name()
            .and_then(|segment| segment.to_str())
            .unwrap_or("cossh-inventory.yaml")
            .to_string();
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

#[derive(Debug, Clone, Default)]
pub(super) struct ParsedInventoryDocument {
    pub include: Vec<String>,
    pub inventory: Vec<InventoryNodeRaw>,
}

#[derive(Debug, Clone)]
pub(super) enum InventoryNodeRaw {
    Host(Box<InventoryHostRaw>),
    Folder { name: String, items: Vec<InventoryNodeRaw> },
}

#[derive(Debug, Clone, Default)]
pub(super) struct InventoryHostRaw {
    pub name: String,
    pub description: Option<String>,
    pub protocol: ConnectionProtocol,
    pub host: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub profile: Option<String>,
    pub vault_pass: Option<String>,
    pub hidden: bool,
    pub identity_files: Vec<String>,
    pub identities_only: Option<bool>,
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
    pub forward_agent: Option<String>,
    pub local_forward: Vec<String>,
    pub remote_forward: Vec<String>,
    pub ssh_options: SshOptionMap,
    pub rdp_domain: Option<String>,
    pub rdp_args: Vec<String>,
}

#[cfg(test)]
#[path = "../test/inventory/model.rs"]
mod tests;
