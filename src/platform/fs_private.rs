//! Shared private filesystem helpers for runtime files and logs.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub(crate) fn create_private_directory(path: &Path, mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_private_directory_permissions(path, mode)
}

pub(crate) fn open_private_append_file(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .create(true) // Create if missing.
        .append(true) // Preserve existing logs.
        .mode(mode);
    let file = options.open(path)?;
    set_private_file_permissions(path, mode)?;
    Ok(file)
}

pub(crate) fn set_private_directory_permissions(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

pub(crate) fn set_private_file_permissions(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
