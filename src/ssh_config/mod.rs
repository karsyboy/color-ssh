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
