//! Host-browser tree and search indexing state.

use crate::ssh_config::FolderId;

#[derive(Debug, Clone, Default)]
pub(crate) struct HostSearchEntry {
    pub(crate) name_lower: String,
    pub(crate) hostname_lower: Option<String>,
    pub(crate) user_lower: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostTreeRowKind {
    Folder(FolderId),
    Host(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct HostTreeRow {
    pub(crate) kind: HostTreeRowKind,
    pub(crate) depth: usize,
    pub(crate) display_name: String,
    pub(crate) expanded: bool,
}
