//! Platform-specific filesystem and executable resolution helpers.

mod command_path;
mod fs_private;

pub(crate) use command_path::{cossh_path, resolve_known_command_path};
pub(crate) use fs_private::{create_private_directory, open_private_append_file, set_private_directory_permissions, set_private_file_permissions};
