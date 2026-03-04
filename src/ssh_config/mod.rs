//! SSH config parsing and include-tree loading.

mod include;
mod model;
mod parser;
mod path;

pub use crate::inventory::{ConnectionProtocol, FolderId, TreeFolder};
/// Parsed host record and tree model types from SSH config.
pub use model::{SshHost, SshHostTreeModel};
/// Parser entrypoints for runtime use and migration use.
pub use parser::{parse_ssh_config, parse_ssh_config_for_migration};
/// Default `~/.ssh/config` discovery helper.
pub use path::get_default_ssh_config_path;

use std::io;

/// Load the SSH include tree rooted at `~/.ssh/config`.
#[allow(dead_code)]
pub(crate) fn load_ssh_host_tree() -> io::Result<SshHostTreeModel> {
    let config_path = get_default_ssh_config_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory"))?;

    if !config_path.exists() {
        return Ok(SshHostTreeModel::empty(config_path));
    }

    parser::build_ssh_host_tree(&config_path)
}
