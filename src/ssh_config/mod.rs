//! SSH config parsing and include-tree loading.

mod include;
mod model;
mod parser;
mod path;

pub use model::{FolderId, SshHost, SshHostTreeModel, TreeFolder};
pub use parser::parse_ssh_config;
pub use path::get_default_ssh_config_path;

use std::io;

/// Load the SSH include tree rooted at `~/.ssh/config`.
pub(crate) fn load_ssh_host_tree() -> io::Result<SshHostTreeModel> {
    let config_path = get_default_ssh_config_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory"))?;

    if !config_path.exists() {
        return Ok(SshHostTreeModel::empty(config_path));
    }

    parser::build_ssh_host_tree(&config_path)
}

/// Load all SSH hosts from the default config.
pub fn load_ssh_hosts() -> io::Result<Vec<SshHost>> {
    Ok(load_ssh_host_tree()?.hosts)
}
