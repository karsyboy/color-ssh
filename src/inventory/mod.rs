//! YAML inventory loading and SSH config migration.

mod include;
mod loader;
mod migration;
mod model;
mod path;

pub use model::{
    ConnectionProtocol, FolderId, InventoryDocumentRaw, InventoryHost, InventoryHostRaw, InventoryNodeRaw, InventoryTreeModel, RdpHostOptions, SshHostOptions,
    SshOptionMap, TreeFolder,
};
pub use path::{expand_tilde, get_default_inventory_path};

use std::io;
use std::path::Path;

pub(crate) use migration::migrate_default_ssh_config_to_inventory;

pub(crate) fn normalize_ssh_forward_spec(value: &str) -> String {
    let trimmed = value.trim();
    let mut parts = trimmed.split_whitespace();
    let Some(left) = parts.next() else {
        return trimmed.to_string();
    };
    let Some(right) = parts.next() else {
        return trimmed.to_string();
    };

    if parts.next().is_some() {
        return trimmed.to_string();
    }

    format!("{left}:{right}")
}

pub(crate) fn build_inventory_tree(inventory_path: &Path) -> io::Result<InventoryTreeModel> {
    loader::build_inventory_tree(inventory_path)
}

pub(crate) fn load_inventory_tree() -> io::Result<InventoryTreeModel> {
    let inventory_path = get_default_inventory_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory"))?;

    if !inventory_path.exists() {
        return Ok(InventoryTreeModel::empty(inventory_path));
    }

    build_inventory_tree(&inventory_path)
}
