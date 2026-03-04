//! Inventory domain models.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Stable folder identifier used by the TUI tree.
pub type FolderId = usize;

pub type SshOptionMap = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionProtocol {
    #[default]
    Ssh,
    Rdp,
    Other(String),
}

impl ConnectionProtocol {
    pub fn from_str(value: &str) -> Self {
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

#[derive(Debug, Clone, Default)]
pub struct InventoryDocumentRaw {
    pub include: Vec<String>,
    pub inventory: Vec<InventoryNodeRaw>,
}

#[derive(Debug, Clone)]
pub enum InventoryNodeRaw {
    Host(InventoryHostRaw),
    Folder { name: String, items: Vec<InventoryNodeRaw> },
}

#[derive(Debug, Clone, Default)]
pub struct InventoryHostRaw {
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

#[derive(Debug, Clone, Default)]
pub struct SshHostOptions {
    pub identity_files: Vec<String>,
    pub identities_only: Option<bool>,
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
    pub forward_agent: Option<String>,
    pub local_forward: Vec<String>,
    pub remote_forward: Vec<String>,
    pub extra_options: SshOptionMap,
}

#[derive(Debug, Clone, Default)]
pub struct RdpHostOptions {
    pub domain: Option<String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InventoryHost {
    pub name: String,
    pub description: Option<String>,
    pub protocol: ConnectionProtocol,
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub profile: Option<String>,
    pub vault_pass: Option<String>,
    pub hidden: bool,
    pub ssh: SshHostOptions,
    pub rdp: RdpHostOptions,
    pub source_file: PathBuf,
    pub source_folder_path: Vec<String>,
}

impl InventoryHost {
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
    pub id: FolderId,
    pub name: String,
    pub path: PathBuf,
    pub children: Vec<TreeFolder>,
    pub host_indices: Vec<usize>,
}

/// Parsed inventory data and folder tree.
#[derive(Debug, Clone)]
pub struct InventoryTreeModel {
    pub root: TreeFolder,
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
