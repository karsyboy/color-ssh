//! TUI state model split by feature domain.

mod app;
mod folder_picker;
mod host_browser;
mod host_editor;
mod quick_connect;
mod rdp_prompt;
mod tabs;
mod vault;

pub(crate) use app::{AppState, ConnectRequest, HOST_PANEL_MAX_WIDTH, HOST_PANEL_MIN_WIDTH};
pub(crate) use folder_picker::{FolderDeleteConfirmState, FolderPickerMode, FolderPickerRow, FolderPickerState, FolderRenameState};
pub(crate) use host_browser::{HostSearchEntry, HostTreeRow, HostTreeRowKind};
pub(crate) use host_editor::{
    HostContextMenuAction, HostContextMenuState, HostContextMenuTarget, HostDeleteConfirmState, HostEditorField, HostEditorMode, HostEditorSection,
    HostEditorState, HostEditorVisibleItem,
};
pub(crate) use quick_connect::{QuickConnectField, QuickConnectRow, QuickConnectState, QuickConnectSubmission, QuickConnectValidationError};
pub(crate) use rdp_prompt::{
    RdpCredentialLaunchContext, RdpCredentialSubmission, RdpCredentialValidationError, RdpCredentialsAction, RdpCredentialsField, RdpCredentialsState,
};
pub(crate) use tabs::{EditorTabId, EditorTabState, HostTab, TerminalSearchState, TerminalTabState};
pub(crate) use vault::{VaultStatusModalState, VaultUnlockAction, VaultUnlockState};
