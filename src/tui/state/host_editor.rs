//! Host editor state used by TUI create/edit/delete workflows.

use crate::args::validate_vault_entry_name;
use crate::inventory::{ConnectionProtocol, EditableInventoryHost, FolderId, InventoryHost, SshOptionMap};
use crate::tui::text_edit;
use serde_yml::{Mapping, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostContextMenuAction {
    EditEntry,
    DuplicateEntry,
    MoveToFolder,
    DeleteEntry,
    Connect,
    NewEntryInFolder,
    RenameFolder,
    DeleteFolder,
}

impl HostContextMenuAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::EditEntry => "Edit Entry",
            Self::DuplicateEntry => "Duplicate Entry",
            Self::MoveToFolder => "Move to Folder...",
            Self::DeleteEntry => "Delete Entry",
            Self::Connect => "Connect",
            Self::NewEntryInFolder => "New Entry in this Folder",
            Self::RenameFolder => "Rename Folder",
            Self::DeleteFolder => "Delete Folder",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum HostContextMenuTarget {
    Host { host_idx: usize },
    Folder { folder_id: FolderId, source_file: PathBuf },
    Background { source_file: PathBuf },
}

#[derive(Debug, Clone)]
pub(crate) struct HostContextMenuState {
    pub(crate) column: u16,
    pub(crate) row: u16,
    pub(crate) actions: Vec<HostContextMenuAction>,
    pub(crate) selected: usize,
    pub(crate) target: HostContextMenuTarget,
}

impl HostContextMenuState {
    pub(crate) fn for_host(column: u16, row: u16, host_idx: usize) -> Self {
        Self {
            column,
            row,
            actions: vec![
                HostContextMenuAction::EditEntry,
                HostContextMenuAction::DuplicateEntry,
                HostContextMenuAction::MoveToFolder,
                HostContextMenuAction::DeleteEntry,
                HostContextMenuAction::Connect,
            ],
            selected: 0,
            target: HostContextMenuTarget::Host { host_idx },
        }
    }

    pub(crate) fn for_folder(column: u16, row: u16, folder_id: FolderId, source_file: PathBuf) -> Self {
        Self {
            column,
            row,
            actions: vec![
                HostContextMenuAction::NewEntryInFolder,
                HostContextMenuAction::RenameFolder,
                HostContextMenuAction::DeleteFolder,
            ],
            selected: 0,
            target: HostContextMenuTarget::Folder { folder_id, source_file },
        }
    }

    pub(crate) fn for_new_entry(column: u16, row: u16, source_file: PathBuf) -> Self {
        Self {
            column,
            row,
            actions: vec![HostContextMenuAction::NewEntryInFolder],
            selected: 0,
            target: HostContextMenuTarget::Background { source_file },
        }
    }

    pub(crate) fn selected_action(&self) -> Option<HostContextMenuAction> {
        self.actions.get(self.selected).copied()
    }

    pub(crate) fn has_action(&self, action: HostContextMenuAction) -> bool {
        self.actions.contains(&action)
    }

    pub(crate) fn select_next(&mut self) {
        if self.actions.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % self.actions.len();
    }

    pub(crate) fn select_prev(&mut self) {
        if self.actions.is_empty() {
            self.selected = 0;
            return;
        }

        if self.selected == 0 {
            self.selected = self.actions.len().saturating_sub(1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub(crate) fn title(&self) -> &'static str {
        match self.target {
            HostContextMenuTarget::Host { .. } => " Host Menu ",
            HostContextMenuTarget::Folder { .. } => " Folder Menu ",
            HostContextMenuTarget::Background { .. } => " Inventory Menu ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostEditorMode {
    Create,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostEditorField {
    Name,
    Description,
    Protocol,
    Host,
    User,
    Port,
    Profile,
    VaultPass,
    IdentityFile,
    IdentitiesOnly,
    ProxyJump,
    ProxyCommand,
    ForwardAgent,
    LocalForward,
    RemoteForward,
    SshOptions,
    RdpDomain,
    RdpArgs,
    FolderPath,
    Save,
    Delete,
    Cancel,
}

impl HostEditorField {
    pub(crate) fn is_action(self) -> bool {
        matches!(self, Self::Save | Self::Delete | Self::Cancel)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Description => "Description",
            Self::Protocol => "Protocol",
            Self::Host => "Host",
            Self::User => "User",
            Self::Port => "Port",
            Self::Profile => "Profile",
            Self::VaultPass => "Vault Pass",
            Self::IdentityFile => "Identity File",
            Self::IdentitiesOnly => "IdentitiesOnly",
            Self::ProxyJump => "Proxy Jump",
            Self::ProxyCommand => "Proxy Command",
            Self::ForwardAgent => "Forward Agent",
            Self::LocalForward => "Local Forward",
            Self::RemoteForward => "Remote Forward",
            Self::SshOptions => "SSH Options",
            Self::RdpDomain => "RDP Domain",
            Self::RdpArgs => "RDP Args",
            Self::FolderPath => "Folder Path",
            Self::Save => "Save Entry",
            Self::Delete => "Delete Entry",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HostEditorSection {
    Basic,
    Authentication,
    ProxyForwarding,
    AdvancedSsh,
    Rdp,
    Placement,
}

impl HostEditorSection {
    const ORDERED: [Self; 6] = [
        Self::Basic,
        Self::Authentication,
        Self::ProxyForwarding,
        Self::AdvancedSsh,
        Self::Rdp,
        Self::Placement,
    ];

    pub(crate) fn ordered() -> &'static [Self] {
        &Self::ORDERED
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Authentication => "Authentication",
            Self::ProxyForwarding => "Proxy & Forwarding",
            Self::AdvancedSsh => "Advanced SSH",
            Self::Rdp => "RDP",
            Self::Placement => "Placement",
        }
    }

    pub(crate) fn is_collapsible(self) -> bool {
        !matches!(self, Self::Basic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostEditorVisibleItem {
    SectionHeader(HostEditorSection),
    Field(HostEditorField),
}

impl HostEditorVisibleItem {
    pub(crate) fn as_field(self) -> Option<HostEditorField> {
        match self {
            Self::Field(field) => Some(field),
            Self::SectionHeader(_) => None,
        }
    }
}

impl From<HostEditorField> for HostEditorVisibleItem {
    fn from(field: HostEditorField) -> Self {
        Self::Field(field)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TextInput {
    pub(crate) value: String,
    pub(crate) cursor: usize,
    pub(crate) selection: Option<(usize, usize)>,
}

impl TextInput {
    fn new(value: String) -> Self {
        let cursor = text_edit::char_len(&value);
        Self {
            value,
            cursor,
            selection: None,
        }
    }

    fn move_left(&mut self) {
        text_edit::clamp_cursor(&self.value, &mut self.cursor);
        let active_selection = text_edit::normalized_selection(&self.value, self.selection);
        self.selection = None;
        if let Some((start, _)) = active_selection {
            self.cursor = start;
        } else if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_right(&mut self) {
        text_edit::clamp_cursor(&self.value, &mut self.cursor);
        let len = text_edit::char_len(&self.value);
        let active_selection = text_edit::normalized_selection(&self.value, self.selection);
        self.selection = None;
        if let Some((_, end)) = active_selection {
            self.cursor = end;
        } else if self.cursor < len {
            self.cursor += 1;
        }
    }

    fn move_home(&mut self) {
        self.cursor = 0;
        self.selection = None;
    }

    fn move_end(&mut self) {
        self.cursor = text_edit::char_len(&self.value);
        self.selection = None;
    }

    fn insert_char(&mut self, ch: char) {
        let _ = text_edit::delete_selection(&mut self.value, &mut self.cursor, &mut self.selection);
        text_edit::clamp_cursor(&self.value, &mut self.cursor);
        let insert_at = text_edit::byte_index_for_char(&self.value, self.cursor);
        self.value.insert(insert_at, ch);
        self.cursor += 1;
        self.selection = None;
    }

    fn backspace(&mut self) {
        if text_edit::delete_selection(&mut self.value, &mut self.cursor, &mut self.selection) {
            return;
        }
        text_edit::clamp_cursor(&self.value, &mut self.cursor);
        if self.cursor == 0 {
            self.selection = None;
            return;
        }

        let end = text_edit::byte_index_for_char(&self.value, self.cursor);
        let start = text_edit::byte_index_for_char(&self.value, self.cursor - 1);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
        self.selection = None;
    }

    fn delete(&mut self) {
        if text_edit::delete_selection(&mut self.value, &mut self.cursor, &mut self.selection) {
            return;
        }
        text_edit::clamp_cursor(&self.value, &mut self.cursor);
        let len = text_edit::char_len(&self.value);
        if self.cursor >= len {
            self.selection = None;
            return;
        }

        let start = text_edit::byte_index_for_char(&self.value, self.cursor);
        let end = text_edit::byte_index_for_char(&self.value, self.cursor + 1);
        self.value.replace_range(start..end, "");
        self.selection = None;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HostEditorState {
    pub(crate) mode: HostEditorMode,
    pub(crate) source_file: PathBuf,
    pub(crate) original_name: Option<String>,
    pub(crate) selected: HostEditorVisibleItem,
    pub(crate) collapsed_sections: HashSet<HostEditorSection>,
    pub(crate) name: TextInput,
    pub(crate) description: TextInput,
    pub(crate) protocol: TextInput,
    pub(crate) host: TextInput,
    pub(crate) user: TextInput,
    pub(crate) port: TextInput,
    pub(crate) profile: TextInput,
    pub(crate) vault_pass: TextInput,
    pub(crate) hidden: bool,
    pub(crate) identity_file: TextInput,
    pub(crate) identities_only: Option<bool>,
    pub(crate) proxy_jump: TextInput,
    pub(crate) proxy_command: TextInput,
    pub(crate) forward_agent: TextInput,
    pub(crate) local_forward: TextInput,
    pub(crate) remote_forward: TextInput,
    pub(crate) ssh_options: TextInput,
    pub(crate) rdp_domain: TextInput,
    pub(crate) rdp_args: TextInput,
    pub(crate) folder_path: TextInput,
    pub(crate) profile_options: Vec<String>,
    pub(crate) profile_index: usize,
    pub(crate) vault_pass_options: Vec<String>,
    pub(crate) vault_pass_index: usize,
    drag_anchor: Option<(HostEditorField, usize)>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HostDeleteConfirmState {
    pub(crate) source_file: PathBuf,
    pub(crate) host_name: String,
    pub(crate) from_editor: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HostEditorSubmission {
    pub(crate) source_file: PathBuf,
    pub(crate) original_name: Option<String>,
    pub(crate) folder_path: Vec<String>,
    pub(crate) host: EditableInventoryHost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostEditorValidationError {
    NameRequired,
    HostRequired,
    ProtocolRequired,
    InvalidProtocol,
    InvalidPort,
    InvalidVaultPass,
    InvalidFolderPath,
    InvalidYamlField { field: &'static str, detail: String },
}

impl HostEditorValidationError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::NameRequired => "Name is required.".to_string(),
            Self::HostRequired => "Host is required.".to_string(),
            Self::ProtocolRequired => "Protocol is required.".to_string(),
            Self::InvalidProtocol => "Protocol must be 'ssh' or 'rdp'.".to_string(),
            Self::InvalidPort => "Port must be a number between 1 and 65535.".to_string(),
            Self::InvalidVaultPass => "Vault Pass is invalid: use only letters, numbers, '.', '_' or '-' .".to_string(),
            Self::InvalidFolderPath => "Folder path must be '/' or use '/folder/subfolder/' format.".to_string(),
            Self::InvalidYamlField { field, detail } => {
                format!("{field} must be valid YAML for this field ({detail}).")
            }
        }
    }
}

impl HostEditorState {
    pub(crate) fn new_create(source_file: PathBuf, profile_options: Vec<String>, vault_pass_options: Vec<String>) -> Self {
        let profile_options = normalize_profile_options(profile_options);
        let vault_pass_options = normalize_cycle_options(vault_pass_options, false);
        let default_profile = initial_profile_value(None, &profile_options);
        let default_vault_pass = initial_vault_pass_value(None, &vault_pass_options);

        let mut state = Self {
            mode: HostEditorMode::Create,
            source_file,
            original_name: None,
            selected: HostEditorVisibleItem::Field(HostEditorField::Name),
            collapsed_sections: default_collapsed_sections(),
            name: TextInput::default(),
            description: TextInput::default(),
            protocol: TextInput::new("ssh".to_string()),
            host: TextInput::default(),
            user: TextInput::default(),
            port: TextInput::new("22".to_string()),
            profile: TextInput::new(default_profile),
            vault_pass: TextInput::new(default_vault_pass),
            hidden: false,
            identity_file: TextInput::default(),
            identities_only: None,
            proxy_jump: TextInput::default(),
            proxy_command: TextInput::default(),
            forward_agent: TextInput::default(),
            local_forward: TextInput::default(),
            remote_forward: TextInput::default(),
            ssh_options: TextInput::default(),
            rdp_domain: TextInput::default(),
            rdp_args: TextInput::default(),
            folder_path: TextInput::new("/".to_string()),
            profile_options,
            profile_index: 0,
            vault_pass_options,
            vault_pass_index: 0,
            drag_anchor: None,
            error: None,
        };
        state.sync_profile_index_from_value();
        state.sync_vault_pass_index_from_value();
        state
    }

    pub(crate) fn new_edit(host: &InventoryHost, profile_options: Vec<String>, vault_pass_options: Vec<String>) -> Self {
        let profile_options = normalize_profile_options(profile_options);
        let vault_pass_options = normalize_cycle_options(vault_pass_options, false);
        let default_profile = initial_profile_value(host.profile.as_deref(), &profile_options);
        let default_vault_pass = initial_vault_pass_value(host.vault_pass.as_deref(), &vault_pass_options);

        let mut state = Self {
            mode: HostEditorMode::Edit,
            source_file: host.source_file.clone(),
            original_name: Some(host.name.clone()),
            selected: HostEditorVisibleItem::Field(HostEditorField::Name),
            collapsed_sections: default_collapsed_sections(),
            name: TextInput::new(host.name.clone()),
            description: TextInput::new(host.description.clone().unwrap_or_default()),
            protocol: TextInput::new(if matches!(host.protocol, ConnectionProtocol::Rdp) {
                "rdp".to_string()
            } else {
                "ssh".to_string()
            }),
            host: TextInput::new(host.host.clone()),
            user: TextInput::new(host.user.clone().unwrap_or_default()),
            port: TextInput::new(host.port.map(|value| value.to_string()).unwrap_or_default()),
            profile: TextInput::new(default_profile),
            vault_pass: TextInput::new(default_vault_pass),
            hidden: host.hidden,
            identity_file: TextInput::new(serialize_yaml_inline(&host.ssh.identity_files)),
            identities_only: host.ssh.identities_only,
            proxy_jump: TextInput::new(host.ssh.proxy_jump.clone().unwrap_or_default()),
            proxy_command: TextInput::new(host.ssh.proxy_command.clone().unwrap_or_default()),
            forward_agent: TextInput::new(host.ssh.forward_agent.clone().unwrap_or_default()),
            local_forward: TextInput::new(serialize_yaml_inline(&host.ssh.local_forward)),
            remote_forward: TextInput::new(serialize_yaml_inline(&host.ssh.remote_forward)),
            ssh_options: TextInput::new(serialize_yaml_inline(&host.ssh.extra_options)),
            rdp_domain: TextInput::new(host.rdp.domain.clone().unwrap_or_default()),
            rdp_args: TextInput::new(serialize_yaml_inline(&host.rdp.args)),
            folder_path: TextInput::new("/".to_string()),
            profile_options,
            profile_index: 0,
            vault_pass_options,
            vault_pass_index: 0,
            drag_anchor: None,
            error: None,
        };
        state.sync_profile_index_from_value();
        state.sync_vault_pass_index_from_value();
        state
    }

    pub(crate) fn new_duplicate(host: &InventoryHost, profile_options: Vec<String>, vault_pass_options: Vec<String>) -> Self {
        let mut state = Self::new_edit(host, profile_options, vault_pass_options);
        state.mode = HostEditorMode::Create;
        state.original_name = None;
        state.selected = HostEditorVisibleItem::Field(HostEditorField::Name);
        state.name = TextInput::new(duplicate_host_name(&host.name));
        state.folder_path = TextInput::new("/".to_string());
        state.error = None;
        state
    }

    pub(crate) fn title(&self) -> &'static str {
        match self.mode {
            HostEditorMode::Create => " New Inventory Entry ",
            HostEditorMode::Edit => " Edit Inventory Entry ",
        }
    }

    pub(crate) fn hint_text(&self) -> &'static str {
        "[←/→] Cycle Protocol/Profile/Vault"
    }

    fn section_template_fields(section: HostEditorSection) -> &'static [HostEditorField] {
        match section {
            HostEditorSection::Basic => &[
                HostEditorField::Name,
                HostEditorField::Description,
                HostEditorField::Protocol,
                HostEditorField::Host,
                HostEditorField::User,
                HostEditorField::Port,
            ],
            HostEditorSection::Authentication => &[
                HostEditorField::Profile,
                HostEditorField::VaultPass,
                HostEditorField::IdentityFile,
                HostEditorField::IdentitiesOnly,
            ],
            HostEditorSection::ProxyForwarding => &[
                HostEditorField::ProxyJump,
                HostEditorField::ProxyCommand,
                HostEditorField::ForwardAgent,
                HostEditorField::LocalForward,
                HostEditorField::RemoteForward,
            ],
            HostEditorSection::AdvancedSsh => &[HostEditorField::SshOptions],
            HostEditorSection::Rdp => &[HostEditorField::RdpDomain, HostEditorField::RdpArgs],
            HostEditorSection::Placement => &[HostEditorField::FolderPath],
        }
    }

    fn section_for_field(field: HostEditorField) -> Option<HostEditorSection> {
        match field {
            HostEditorField::Name
            | HostEditorField::Description
            | HostEditorField::Protocol
            | HostEditorField::Host
            | HostEditorField::User
            | HostEditorField::Port => Some(HostEditorSection::Basic),
            HostEditorField::Profile | HostEditorField::VaultPass | HostEditorField::IdentityFile | HostEditorField::IdentitiesOnly => {
                Some(HostEditorSection::Authentication)
            }
            HostEditorField::ProxyJump
            | HostEditorField::ProxyCommand
            | HostEditorField::ForwardAgent
            | HostEditorField::LocalForward
            | HostEditorField::RemoteForward => Some(HostEditorSection::ProxyForwarding),
            HostEditorField::SshOptions => Some(HostEditorSection::AdvancedSsh),
            HostEditorField::RdpDomain | HostEditorField::RdpArgs => Some(HostEditorSection::Rdp),
            HostEditorField::FolderPath => Some(HostEditorSection::Placement),
            HostEditorField::Save | HostEditorField::Delete | HostEditorField::Cancel => None,
        }
    }

    fn field_matches_context(&self, field: HostEditorField) -> bool {
        match field {
            HostEditorField::Profile
            | HostEditorField::IdentityFile
            | HostEditorField::IdentitiesOnly
            | HostEditorField::ProxyJump
            | HostEditorField::ProxyCommand
            | HostEditorField::ForwardAgent
            | HostEditorField::LocalForward
            | HostEditorField::RemoteForward
            | HostEditorField::SshOptions => !self.is_rdp(),
            HostEditorField::RdpDomain | HostEditorField::RdpArgs => self.is_rdp(),
            HostEditorField::FolderPath => self.mode == HostEditorMode::Create,
            _ => true,
        }
    }

    fn visible_section_fields(&self, section: HostEditorSection) -> Vec<HostEditorField> {
        Self::section_template_fields(section)
            .iter()
            .copied()
            .filter(|field| self.field_matches_context(*field))
            .collect()
    }

    pub(crate) fn visible_sections(&self) -> Vec<HostEditorSection> {
        HostEditorSection::ordered()
            .iter()
            .copied()
            .filter(|section| !self.visible_section_fields(*section).is_empty())
            .collect()
    }

    pub(crate) fn section_collapsed(&self, section: HostEditorSection) -> bool {
        section.is_collapsible() && self.collapsed_sections.contains(&section)
    }

    pub(crate) fn toggle_section(&mut self, section: HostEditorSection) {
        if !section.is_collapsible() || !self.visible_sections().contains(&section) {
            return;
        }

        if self.section_collapsed(section) {
            self.collapsed_sections.remove(&section);
        } else {
            self.collapsed_sections.insert(section);
        }

        self.ensure_selected_item_visible();
    }

    pub(crate) fn visible_items(&self) -> Vec<HostEditorVisibleItem> {
        let mut items = Vec::new();
        for section in self.visible_sections() {
            items.push(HostEditorVisibleItem::SectionHeader(section));
            if self.section_collapsed(section) {
                continue;
            }
            items.extend(self.visible_section_fields(section).into_iter().map(HostEditorVisibleItem::Field));
        }

        items.push(HostEditorVisibleItem::Field(HostEditorField::Save));
        if self.mode == HostEditorMode::Edit {
            items.push(HostEditorVisibleItem::Field(HostEditorField::Delete));
        }
        items.push(HostEditorVisibleItem::Field(HostEditorField::Cancel));

        items
    }

    pub(crate) fn visible_fields(&self) -> Vec<HostEditorField> {
        self.visible_items().into_iter().filter_map(HostEditorVisibleItem::as_field).collect()
    }

    pub(crate) fn selected_field(&self) -> Option<HostEditorField> {
        self.selected.as_field()
    }

    pub(crate) fn is_selected_field(&self, field: HostEditorField) -> bool {
        self.selected == HostEditorVisibleItem::Field(field)
    }

    pub(crate) fn is_selected_section(&self, section: HostEditorSection) -> bool {
        self.selected == HostEditorVisibleItem::SectionHeader(section)
    }

    pub(crate) fn is_rdp(&self) -> bool {
        self.protocol.value.trim().eq_ignore_ascii_case("rdp")
    }

    pub(crate) fn toggle_protocol_forward(&mut self) {
        let was_rdp = self.is_rdp();
        self.set_protocol_value(if was_rdp { "ssh" } else { "rdp" });
        self.apply_default_port_for_protocol_switch(was_rdp);
    }

    pub(crate) fn toggle_protocol_backward(&mut self) {
        self.toggle_protocol_forward();
    }

    pub(crate) fn select_next_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.profile_options.len();
        self.profile = TextInput::new(self.profile_options[self.profile_index].clone());
    }

    pub(crate) fn select_prev_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        if self.profile_index == 0 {
            self.profile_index = self.profile_options.len().saturating_sub(1);
        } else {
            self.profile_index = self.profile_index.saturating_sub(1);
        }
        self.profile = TextInput::new(self.profile_options[self.profile_index].clone());
    }

    pub(crate) fn select_next_vault_pass(&mut self) {
        self.finish_mouse_selection();
        if self.vault_pass_options.is_empty() {
            return;
        }
        self.vault_pass_index = (self.vault_pass_index + 1) % self.vault_pass_options.len();
        self.vault_pass = TextInput::new(self.vault_pass_options[self.vault_pass_index].clone());
    }

    pub(crate) fn select_prev_vault_pass(&mut self) {
        self.finish_mouse_selection();
        if self.vault_pass_options.is_empty() {
            return;
        }
        if self.vault_pass_index == 0 {
            self.vault_pass_index = self.vault_pass_options.len().saturating_sub(1);
        } else {
            self.vault_pass_index = self.vault_pass_index.saturating_sub(1);
        }
        self.vault_pass = TextInput::new(self.vault_pass_options[self.vault_pass_index].clone());
    }

    #[cfg(test)]
    pub(crate) fn modal_height(&self) -> u16 {
        let visible_items = self.visible_items();
        let non_action_rows = visible_items
            .iter()
            .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
            .count() as u16;
        let action_rows = if visible_items
            .iter()
            .any(|item| matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
        {
            1
        } else {
            0
        };
        let field_rows = non_action_rows.saturating_add(action_rows);
        // file row + spacer + message + hint + action row + borders
        field_rows.saturating_add(7)
    }

    pub(crate) fn select_next_field(&mut self) {
        self.selected = self.next_visible_item_from(self.selected);
    }

    pub(crate) fn select_prev_field(&mut self) {
        self.selected = self.prev_visible_item_from(self.selected);
    }

    fn next_visible_item_from(&self, item: HostEditorVisibleItem) -> HostEditorVisibleItem {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return HostEditorVisibleItem::Field(HostEditorField::Save);
        }

        if let Some(idx) = visible_items.iter().position(|candidate| *candidate == item) {
            let next_idx = (idx + 1) % visible_items.len();
            return visible_items[next_idx];
        }

        visible_items[0]
    }

    fn prev_visible_item_from(&self, item: HostEditorVisibleItem) -> HostEditorVisibleItem {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return HostEditorVisibleItem::Field(HostEditorField::Save);
        }

        if let Some(idx) = visible_items.iter().position(|candidate| *candidate == item) {
            let prev_idx = if idx == 0 {
                visible_items.len().saturating_sub(1)
            } else {
                idx.saturating_sub(1)
            };
            return visible_items[prev_idx];
        }

        visible_items[0]
    }

    fn ensure_selected_item_visible(&mut self) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            self.selected = HostEditorVisibleItem::Field(HostEditorField::Save);
            return;
        }

        if visible_items.contains(&self.selected) {
            return;
        }

        if let Some(field) = self.selected_field()
            && let Some(section) = Self::section_for_field(field)
            && visible_items.contains(&HostEditorVisibleItem::SectionHeader(section))
        {
            self.selected = HostEditorVisibleItem::SectionHeader(section);
            return;
        }

        if visible_items.contains(&HostEditorVisibleItem::Field(HostEditorField::Protocol)) {
            self.selected = HostEditorVisibleItem::Field(HostEditorField::Protocol);
        } else {
            self.selected = visible_items[0];
        }
    }

    fn set_protocol_value(&mut self, protocol: &str) {
        self.protocol = TextInput::new(protocol.to_string());
        self.ensure_selected_item_visible();
    }

    fn apply_default_port_for_protocol_switch(&mut self, was_rdp: bool) {
        let previous_default = if was_rdp { "3389" } else { "22" };
        let next_default = if self.is_rdp() { "3389" } else { "22" };
        let current = self.port.value.trim();

        if current.is_empty() || current == previous_default {
            self.port = TextInput::new(next_default.to_string());
        }
    }

    fn sync_profile_index_from_value(&mut self) {
        let selected = self.profile.value.trim();
        if let Some(idx) = self.profile_options.iter().position(|profile| profile.eq_ignore_ascii_case(selected)) {
            self.profile_index = idx;
        } else if !self.profile_options.is_empty() {
            self.profile_index = 0;
        }
    }

    fn sync_vault_pass_index_from_value(&mut self) {
        let selected = self.vault_pass.value.trim();
        if let Some(idx) = self.vault_pass_options.iter().position(|entry| entry.eq_ignore_ascii_case(selected)) {
            self.vault_pass_index = idx;
        } else if !self.vault_pass_options.is_empty() {
            self.vault_pass_index = 0;
        }
    }

    pub(crate) fn identities_only_display(&self) -> &'static str {
        match self.identities_only {
            None => "auto",
            Some(true) => "yes",
            Some(false) => "no",
        }
    }

    pub(crate) fn field_example(&self, field: HostEditorField) -> Option<&'static str> {
        match field {
            HostEditorField::Name => None,
            HostEditorField::Description => None,
            HostEditorField::Protocol => None,
            HostEditorField::Host => None,
            HostEditorField::User => None,
            HostEditorField::Port => Some("22 or 3389"),
            HostEditorField::Profile => None,
            HostEditorField::VaultPass => None,
            HostEditorField::IdentityFile => Some("[\"~/.ssh/id_ed25519\"]"),
            HostEditorField::IdentitiesOnly => Some("auto | yes | no"),
            HostEditorField::ProxyJump => Some("jump.example.com"),
            HostEditorField::ProxyCommand => Some("ssh -W %h:%p bastion"),
            HostEditorField::ForwardAgent => Some("yes | no"),
            HostEditorField::LocalForward => Some("[\"127.0.0.1:8080:localhost:80\"]"),
            HostEditorField::RemoteForward => Some("[\"0.0.0.0:2222:localhost:22\"]"),
            HostEditorField::SshOptions => Some("{StrictHostKeyChecking: ask}"),
            HostEditorField::RdpDomain => None,
            HostEditorField::RdpArgs => Some("[\"/cert:ignore\"]"),
            HostEditorField::FolderPath => Some("/folder/folder1/"),
            HostEditorField::Save | HostEditorField::Delete | HostEditorField::Cancel => None,
        }
    }

    pub(crate) fn cycle_identities_only_forward(&mut self) {
        self.identities_only = match self.identities_only {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
    }

    pub(crate) fn cycle_identities_only_backward(&mut self) {
        self.identities_only = match self.identities_only {
            None => Some(false),
            Some(false) => Some(true),
            Some(true) => None,
        };
    }

    fn text_field_mut(&mut self, field: HostEditorField) -> Option<&mut TextInput> {
        match field {
            HostEditorField::Name => Some(&mut self.name),
            HostEditorField::Description => Some(&mut self.description),
            HostEditorField::Protocol => Some(&mut self.protocol),
            HostEditorField::Host => Some(&mut self.host),
            HostEditorField::User => Some(&mut self.user),
            HostEditorField::Port => Some(&mut self.port),
            HostEditorField::Profile => Some(&mut self.profile),
            HostEditorField::VaultPass => Some(&mut self.vault_pass),
            HostEditorField::IdentityFile => Some(&mut self.identity_file),
            HostEditorField::ProxyJump => Some(&mut self.proxy_jump),
            HostEditorField::ProxyCommand => Some(&mut self.proxy_command),
            HostEditorField::ForwardAgent => Some(&mut self.forward_agent),
            HostEditorField::LocalForward => Some(&mut self.local_forward),
            HostEditorField::RemoteForward => Some(&mut self.remote_forward),
            HostEditorField::SshOptions => Some(&mut self.ssh_options),
            HostEditorField::RdpDomain => Some(&mut self.rdp_domain),
            HostEditorField::RdpArgs => Some(&mut self.rdp_args),
            HostEditorField::FolderPath => Some(&mut self.folder_path),
            _ => None,
        }
    }

    pub(crate) fn text_field(&self, field: HostEditorField) -> Option<&TextInput> {
        match field {
            HostEditorField::Name => Some(&self.name),
            HostEditorField::Description => Some(&self.description),
            HostEditorField::Protocol => Some(&self.protocol),
            HostEditorField::Host => Some(&self.host),
            HostEditorField::User => Some(&self.user),
            HostEditorField::Port => Some(&self.port),
            HostEditorField::Profile => Some(&self.profile),
            HostEditorField::VaultPass => Some(&self.vault_pass),
            HostEditorField::IdentityFile => Some(&self.identity_file),
            HostEditorField::ProxyJump => Some(&self.proxy_jump),
            HostEditorField::ProxyCommand => Some(&self.proxy_command),
            HostEditorField::ForwardAgent => Some(&self.forward_agent),
            HostEditorField::LocalForward => Some(&self.local_forward),
            HostEditorField::RemoteForward => Some(&self.remote_forward),
            HostEditorField::SshOptions => Some(&self.ssh_options),
            HostEditorField::RdpDomain => Some(&self.rdp_domain),
            HostEditorField::RdpArgs => Some(&self.rdp_args),
            HostEditorField::FolderPath => Some(&self.folder_path),
            _ => None,
        }
    }

    pub(crate) fn begin_mouse_selection(&mut self, field: HostEditorField, column: usize) {
        self.drag_anchor = None;
        let mut anchor = None;
        if let Some(input) = self.text_field_mut(field) {
            let next_cursor = column.min(text_edit::char_len(&input.value));
            input.cursor = next_cursor;
            input.selection = None;
            anchor = Some(next_cursor);
        }
        if let Some(anchor) = anchor {
            self.drag_anchor = Some((field, anchor));
        }
    }

    pub(crate) fn extend_mouse_selection(&mut self, column: usize) {
        let Some((field, anchor)) = self.drag_anchor else {
            return;
        };

        if let Some(input) = self.text_field_mut(field) {
            let next_cursor = column.min(text_edit::char_len(&input.value));
            input.cursor = next_cursor;
            input.selection = if next_cursor == anchor { None } else { Some((anchor, next_cursor)) };
        }
    }

    pub(crate) fn mouse_drag_field(&self) -> Option<HostEditorField> {
        self.drag_anchor.map(|(field, _)| field)
    }

    pub(crate) fn finish_mouse_selection(&mut self) {
        self.drag_anchor = None;
    }

    pub(crate) fn move_cursor_left(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.move_left();
        }
    }

    pub(crate) fn move_cursor_right(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.move_right();
        }
    }

    pub(crate) fn move_cursor_home(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.move_home();
        }
    }

    pub(crate) fn move_cursor_end(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.move_end();
        }
    }

    pub(crate) fn insert_char(&mut self, field: HostEditorField, ch: char) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.insert_char(ch);
        }
        match field {
            HostEditorField::Profile => self.sync_profile_index_from_value(),
            HostEditorField::VaultPass => self.sync_vault_pass_index_from_value(),
            _ => {}
        }
    }

    pub(crate) fn backspace(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.backspace();
        }
        match field {
            HostEditorField::Profile => self.sync_profile_index_from_value(),
            HostEditorField::VaultPass => self.sync_vault_pass_index_from_value(),
            _ => {}
        }
    }

    pub(crate) fn delete(&mut self, field: HostEditorField) {
        self.finish_mouse_selection();
        if let Some(input) = self.text_field_mut(field) {
            input.delete();
        }
        match field {
            HostEditorField::Profile => self.sync_profile_index_from_value(),
            HostEditorField::VaultPass => self.sync_vault_pass_index_from_value(),
            _ => {}
        }
    }

    pub(crate) fn cursor_for_field(&self, field: HostEditorField) -> Option<usize> {
        self.text_field(field).map(|input| input.cursor)
    }

    pub(crate) fn selection_for_field(&self, field: HostEditorField) -> Option<(usize, usize)> {
        self.text_field(field)
            .and_then(|input| text_edit::normalized_selection(&input.value, input.selection))
    }

    pub(crate) fn field_horizontal_scroll_offset(&self, field: HostEditorField, value_width: u16) -> usize {
        let Some(input) = self.text_field(field) else {
            return 0;
        };

        if !self.is_selected_field(field) {
            return 0;
        }

        let len = text_edit::char_len(&input.value);
        let cursor = input.cursor.min(len);
        let mut visible_chars = value_width as usize;

        // Keep a cell available for the trailing cursor block when caret is at EOL.
        if cursor == len && visible_chars > 0 {
            visible_chars = visible_chars.saturating_sub(1);
        }

        if len == 0 || visible_chars == 0 || len <= visible_chars {
            return 0;
        }

        let mut start = if cursor >= visible_chars {
            cursor.saturating_add(1).saturating_sub(visible_chars)
        } else {
            0
        };

        let max_start = len.saturating_sub(visible_chars);
        start = start.min(max_start);
        start
    }

    pub(crate) fn build_submission(&self) -> Result<HostEditorSubmission, HostEditorValidationError> {
        let name = self.name.value.trim();
        if name.is_empty() {
            return Err(HostEditorValidationError::NameRequired);
        }

        let host = self.host.value.trim();
        if host.is_empty() {
            return Err(HostEditorValidationError::HostRequired);
        }

        let protocol = self.protocol.value.trim();
        if protocol.is_empty() {
            return Err(HostEditorValidationError::ProtocolRequired);
        }

        let protocol = match protocol.to_ascii_lowercase().as_str() {
            "ssh" => ConnectionProtocol::Ssh,
            "rdp" => ConnectionProtocol::Rdp,
            _ => return Err(HostEditorValidationError::InvalidProtocol),
        };
        let is_rdp = matches!(protocol, ConnectionProtocol::Rdp);

        let port = if self.port.value.trim().is_empty() {
            None
        } else {
            Some(self.port.value.trim().parse::<u16>().map_err(|_| HostEditorValidationError::InvalidPort)?)
        };

        let vault_pass = optional_trimmed_string(&self.vault_pass.value);
        if let Some(vault_pass) = vault_pass.as_deref()
            && !validate_vault_entry_name(vault_pass)
        {
            return Err(HostEditorValidationError::InvalidVaultPass);
        }

        let folder_path = if self.mode == HostEditorMode::Create {
            parse_folder_path(self.folder_path.value.trim())?
        } else {
            Vec::new()
        };

        let editable_host = EditableInventoryHost {
            name: name.to_string(),
            description: optional_trimmed_string(&self.description.value),
            protocol: protocol.clone(),
            host: host.to_string(),
            user: optional_trimmed_string(&self.user.value),
            port,
            profile: if is_rdp { None } else { optional_trimmed_profile(&self.profile.value) },
            vault_pass,
            hidden: self.hidden,
            ssh_identity_files: if is_rdp {
                Vec::new()
            } else {
                parse_yaml_string_list_field(&self.identity_file.value, "Identity File")?
            },
            ssh_identities_only: if is_rdp { None } else { self.identities_only },
            ssh_proxy_jump: if is_rdp { None } else { optional_trimmed_string(&self.proxy_jump.value) },
            ssh_proxy_command: if is_rdp { None } else { optional_trimmed_string(&self.proxy_command.value) },
            ssh_forward_agent: if is_rdp { None } else { optional_trimmed_string(&self.forward_agent.value) },
            ssh_local_forward: if is_rdp {
                Vec::new()
            } else {
                parse_yaml_string_list_field(&self.local_forward.value, "Local Forward")?
            },
            ssh_remote_forward: if is_rdp {
                Vec::new()
            } else {
                parse_yaml_string_list_field(&self.remote_forward.value, "Remote Forward")?
            },
            ssh_options: if is_rdp {
                BTreeMap::new()
            } else {
                parse_yaml_ssh_options_field(&self.ssh_options.value)?
            },
            rdp_domain: if is_rdp { optional_trimmed_string(&self.rdp_domain.value) } else { None },
            rdp_args: if is_rdp {
                parse_yaml_string_list_field(&self.rdp_args.value, "RDP Args")?
            } else {
                Vec::new()
            },
        };

        Ok(HostEditorSubmission {
            source_file: self.source_file.clone(),
            original_name: self.original_name.clone(),
            folder_path,
            host: editable_host,
        })
    }
}

fn default_collapsed_sections() -> HashSet<HostEditorSection> {
    HostEditorSection::ordered()
        .iter()
        .copied()
        .filter(|section| section.is_collapsible() && !matches!(section, HostEditorSection::Authentication))
        .collect()
}

fn duplicate_host_name(source_name: &str) -> String {
    let trimmed = source_name.trim();
    if trimmed.is_empty() {
        "entry (copy)".to_string()
    } else {
        format!("{trimmed} (copy)")
    }
}

fn optional_trimmed_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn optional_trimmed_profile(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("default") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_profile_options(profile_options: Vec<String>) -> Vec<String> {
    let mut normalized = normalize_cycle_options(profile_options, false);

    if let Some(default_idx) = normalized.iter().position(|profile| profile.eq_ignore_ascii_case("default")) {
        if default_idx != 0 {
            let default_profile = normalized.remove(default_idx);
            normalized.insert(0, default_profile);
        }
    } else {
        normalized.insert(0, "default".to_string());
    }

    normalized
}

fn normalize_cycle_options(options: Vec<String>, include_empty: bool) -> Vec<String> {
    let mut normalized = Vec::new();
    if include_empty {
        normalized.push(String::new());
    }

    for option in options {
        let trimmed = option.trim();
        if trimmed.is_empty() {
            continue;
        }

        if normalized.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }

    if normalized.is_empty() && include_empty {
        normalized.push(String::new());
    }

    normalized
}

fn initial_profile_value(current: Option<&str>, profile_options: &[String]) -> String {
    if let Some(current) = current.map(str::trim).filter(|value| !value.is_empty()) {
        return current.to_string();
    }

    if let Some(default_profile) = profile_options.iter().find(|profile| profile.eq_ignore_ascii_case("default")) {
        return default_profile.clone();
    }

    profile_options.first().cloned().unwrap_or_default()
}

fn initial_vault_pass_value(current: Option<&str>, vault_pass_options: &[String]) -> String {
    if let Some(current) = current.map(str::trim).filter(|value| !value.is_empty()) {
        return current.to_string();
    }

    vault_pass_options.first().cloned().unwrap_or_default()
}

pub(crate) fn parse_folder_path(path: &str) -> Result<Vec<String>, HostEditorValidationError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Ok(Vec::new());
    }

    if !trimmed.starts_with('/') || !trimmed.ends_with('/') {
        return Err(HostEditorValidationError::InvalidFolderPath);
    }

    let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    let segments = inner.split('/').map(str::trim).map(str::to_string).collect::<Vec<_>>();

    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty() || segment.contains('/')) {
        return Err(HostEditorValidationError::InvalidFolderPath);
    }

    Ok(segments)
}

fn parse_yaml_value(raw: &str, field: &'static str) -> Result<Value, HostEditorValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }

    serde_yml::from_str::<Value>(trimmed).map_err(|err| HostEditorValidationError::InvalidYamlField {
        field,
        detail: err.to_string(),
    })
}

fn parse_yaml_string_list_field(raw: &str, field: &'static str) -> Result<Vec<String>, HostEditorValidationError> {
    let value = parse_yaml_value(raw, field)?;
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Sequence(sequence) => sequence.iter().map(|item| yaml_scalar_to_string(item, field)).collect(),
        other => Ok(vec![yaml_scalar_to_string(&other, field)?]),
    }
}

fn parse_yaml_ssh_options_field(raw: &str) -> Result<SshOptionMap, HostEditorValidationError> {
    let value = parse_yaml_value(raw, "SSH Options")?;
    match value {
        Value::Null => Ok(BTreeMap::new()),
        Value::Mapping(mapping) => parse_ssh_options_map(&mapping),
        _ => Err(HostEditorValidationError::InvalidYamlField {
            field: "SSH Options",
            detail: "value must be a YAML mapping".to_string(),
        }),
    }
}

fn parse_ssh_options_map(mapping: &Mapping) -> Result<SshOptionMap, HostEditorValidationError> {
    let mut options = BTreeMap::new();

    for (key, value) in mapping {
        let key = yaml_scalar_to_string(key, "SSH Options")?;
        let values = parse_ssh_option_values(value)?;
        if !values.is_empty() {
            options.insert(key, values);
        }
    }

    Ok(options)
}

fn parse_ssh_option_values(value: &Value) -> Result<Vec<String>, HostEditorValidationError> {
    match value {
        Value::Null => Err(HostEditorValidationError::InvalidYamlField {
            field: "SSH Options",
            detail: "null values are not allowed".to_string(),
        }),
        Value::Sequence(sequence) => sequence.iter().map(ssh_option_scalar_to_string).collect(),
        scalar => Ok(vec![ssh_option_scalar_to_string(scalar)?]),
    }
}

fn yaml_scalar_to_string(value: &Value, field: &'static str) -> Result<String, HostEditorValidationError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Null => Err(HostEditorValidationError::InvalidYamlField {
            field,
            detail: "null values are not allowed".to_string(),
        }),
        _ => Err(HostEditorValidationError::InvalidYamlField {
            field,
            detail: "value must be a scalar string/number/boolean".to_string(),
        }),
    }
}

fn ssh_option_scalar_to_string(value: &Value) -> Result<String, HostEditorValidationError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Bool(boolean) => Ok(if *boolean { "yes".to_string() } else { "no".to_string() }),
        Value::Number(number) => Ok(number.to_string()),
        Value::Null => Err(HostEditorValidationError::InvalidYamlField {
            field: "SSH Options",
            detail: "null values are not allowed".to_string(),
        }),
        _ => Err(HostEditorValidationError::InvalidYamlField {
            field: "SSH Options",
            detail: "value must be a scalar string/number/boolean".to_string(),
        }),
    }
}

fn serialize_yaml_inline<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
