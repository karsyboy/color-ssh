use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) type InventoryResult<T> = Result<T, InventoryError>;

#[derive(Debug, Clone)]
pub(crate) struct InventoryError {
    source_file: PathBuf,
    message: String,
}

impl InventoryError {
    pub(crate) fn new(source_file: &Path, message: impl Into<String>) -> Self {
        Self {
            source_file: source_file.to_path_buf(),
            message: message.into(),
        }
    }
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inventory error in '{}': {}", self.source_file.display(), self.message)
    }
}

impl std::error::Error for InventoryError {}

impl From<InventoryError> for io::Error {
    fn from(value: InventoryError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, value.to_string())
    }
}

pub(crate) fn invalid_inventory(source_file: &Path, message: impl Into<String>) -> InventoryError {
    InventoryError::new(source_file, message)
}
