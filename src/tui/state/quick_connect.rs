//! Quick-connect form state.

use crate::auth::secret::{SensitiveBuffer, SensitiveString};
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::tui::text_edit;

type TextSelection = Option<(usize, usize)>;
type TextCursorSelectionMut<'a> = (&'a mut String, &'a mut usize, &'a mut TextSelection);
type TextCursorSelection<'a> = (&'a str, usize, TextSelection);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectField {
    Protocol,
    User,
    Host,
    Port,
    Domain,
    Password,
    Profile,
    Logging,
    Connect,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectRow {
    Field(QuickConnectField),
    ProfileOptions,
    Message,
    Actions,
}

impl QuickConnectField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Protocol => Self::User,
            Self::User => Self::Host,
            Self::Host => Self::Port,
            Self::Port => Self::Domain,
            Self::Domain => Self::Password,
            Self::Password => Self::Profile,
            Self::Profile => Self::Logging,
            Self::Logging => Self::Connect,
            Self::Connect => Self::Cancel,
            Self::Cancel => Self::Protocol,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::Protocol => Self::Cancel,
            Self::User => Self::Protocol,
            Self::Host => Self::User,
            Self::Port => Self::Host,
            Self::Domain => Self::Port,
            Self::Password => Self::Domain,
            Self::Profile => Self::Password,
            Self::Logging => Self::Profile,
            Self::Connect => Self::Logging,
            Self::Cancel => Self::Connect,
        }
    }
}

#[derive(Debug)]
pub(crate) struct QuickConnectSubmission {
    pub(crate) host: InventoryHost,
    pub(crate) force_ssh_logging: bool,
    pub(crate) manual_rdp_password: Option<SensitiveString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuickConnectValidationError {
    HostRequired,
    RdpUserRequired,
    InvalidPort,
    PasswordEncoding(String),
}

impl QuickConnectValidationError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::HostRequired => "Host is required.".to_string(),
            Self::RdpUserRequired => "RDP quick connect requires a username.".to_string(),
            Self::InvalidPort => "Port must be a number between 1 and 65535.".to_string(),
            Self::PasswordEncoding(detail) => format!("RDP password could not be processed ({detail})."),
        }
    }
}

#[derive(Debug)]
pub(crate) struct QuickConnectState {
    pub(crate) protocol: ConnectionProtocol,
    pub(crate) user: String,
    pub(crate) user_cursor: usize,
    pub(crate) user_selection: Option<(usize, usize)>,
    pub(crate) host: String,
    pub(crate) host_cursor: usize,
    pub(crate) host_selection: Option<(usize, usize)>,
    pub(crate) port: String,
    pub(crate) port_cursor: usize,
    pub(crate) port_selection: Option<(usize, usize)>,
    pub(crate) domain: String,
    pub(crate) domain_cursor: usize,
    pub(crate) domain_selection: Option<(usize, usize)>,
    pub(crate) password: SensitiveBuffer,
    pub(crate) password_cursor: usize,
    drag_anchor: Option<(QuickConnectField, usize)>,
    pub(crate) profile_options: Vec<String>,
    pub(crate) profile_index: usize,
    pub(crate) ssh_logging: bool,
    pub(crate) selected: QuickConnectField,
    pub(crate) host_required: bool,
    pub(crate) error: Option<String>,
}

impl QuickConnectState {
    pub(crate) fn new(default_ssh_logging: bool, mut profile_options: Vec<String>) -> Self {
        if profile_options.is_empty() {
            profile_options.push("default".to_string());
        }
        let profile_index = profile_options.iter().position(|profile| profile.eq_ignore_ascii_case("default")).unwrap_or(0);

        Self {
            protocol: ConnectionProtocol::Ssh,
            user: String::new(),
            user_cursor: 0,
            user_selection: None,
            host: String::new(),
            host_cursor: 0,
            host_selection: None,
            port: String::new(),
            port_cursor: 0,
            port_selection: None,
            domain: String::new(),
            domain_cursor: 0,
            domain_selection: None,
            password: SensitiveBuffer::new(),
            password_cursor: 0,
            drag_anchor: None,
            profile_options,
            profile_index,
            ssh_logging: default_ssh_logging,
            selected: QuickConnectField::Protocol,
            host_required: false,
            error: None,
        }
    }

    pub(crate) fn protocol_display_name(&self) -> &'static str {
        match self.protocol {
            ConnectionProtocol::Ssh => "SSH",
            ConnectionProtocol::Rdp => "RDP",
            ConnectionProtocol::Other(_) => "Other",
        }
    }

    pub(crate) fn is_rdp(&self) -> bool {
        matches!(self.protocol, ConnectionProtocol::Rdp)
    }

    pub(crate) fn field_visible(&self, field: QuickConnectField) -> bool {
        match field {
            QuickConnectField::Port | QuickConnectField::Domain | QuickConnectField::Password => self.is_rdp(),
            QuickConnectField::Profile | QuickConnectField::Logging => !self.is_rdp(),
            _ => true,
        }
    }

    pub(crate) fn modal_rows(&self) -> Vec<QuickConnectRow> {
        let mut rows = vec![
            QuickConnectRow::Field(QuickConnectField::Protocol),
            QuickConnectRow::Field(QuickConnectField::User),
            QuickConnectRow::Field(QuickConnectField::Host),
        ];

        if self.is_rdp() {
            rows.extend([
                QuickConnectRow::Field(QuickConnectField::Port),
                QuickConnectRow::Field(QuickConnectField::Domain),
                QuickConnectRow::Field(QuickConnectField::Password),
            ]);
        }

        if !self.is_rdp() {
            rows.push(QuickConnectRow::Field(QuickConnectField::Profile));
            rows.push(QuickConnectRow::ProfileOptions);
            rows.push(QuickConnectRow::Field(QuickConnectField::Logging));
        }

        rows.push(QuickConnectRow::Message);
        rows.push(QuickConnectRow::Actions);
        rows
    }

    pub(crate) fn modal_height(&self) -> u16 {
        self.modal_rows().len() as u16 + 2
    }

    pub(crate) fn masked_password(&self) -> String {
        self.password.masked()
    }

    pub(crate) fn toggle_protocol_forward(&mut self) {
        self.finish_mouse_selection();
        self.protocol = match self.protocol {
            ConnectionProtocol::Ssh => ConnectionProtocol::Rdp,
            ConnectionProtocol::Rdp | ConnectionProtocol::Other(_) => ConnectionProtocol::Ssh,
        };
        self.ensure_selected_field_visible();
        self.error = None;
    }

    pub(crate) fn toggle_protocol_backward(&mut self) {
        self.toggle_protocol_forward();
    }

    pub(crate) fn select_next_field(&mut self) {
        self.selected = self.next_visible_field_from(self.selected);
    }

    pub(crate) fn select_prev_field(&mut self) {
        self.selected = self.prev_visible_field_from(self.selected);
    }

    fn next_visible_field_from(&self, field: QuickConnectField) -> QuickConnectField {
        let mut next = field.next();
        while !self.field_visible(next) {
            next = next.next();
        }
        next
    }

    fn prev_visible_field_from(&self, field: QuickConnectField) -> QuickConnectField {
        let mut prev = field.prev();
        while !self.field_visible(prev) {
            prev = prev.prev();
        }
        prev
    }

    fn ensure_selected_field_visible(&mut self) {
        if !self.field_visible(self.selected) {
            self.selected = QuickConnectField::Protocol;
        }
    }

    fn text_cursor_selection_mut(&mut self, field: QuickConnectField) -> Option<TextCursorSelectionMut<'_>> {
        match field {
            QuickConnectField::User => Some((&mut self.user, &mut self.user_cursor, &mut self.user_selection)),
            QuickConnectField::Host => Some((&mut self.host, &mut self.host_cursor, &mut self.host_selection)),
            QuickConnectField::Port => Some((&mut self.port, &mut self.port_cursor, &mut self.port_selection)),
            QuickConnectField::Domain => Some((&mut self.domain, &mut self.domain_cursor, &mut self.domain_selection)),
            _ => None,
        }
    }

    fn text_cursor_selection(&self, field: QuickConnectField) -> Option<TextCursorSelection<'_>> {
        match field {
            QuickConnectField::User => Some((self.user.as_str(), self.user_cursor, self.user_selection)),
            QuickConnectField::Host => Some((self.host.as_str(), self.host_cursor, self.host_selection)),
            QuickConnectField::Port => Some((self.port.as_str(), self.port_cursor, self.port_selection)),
            QuickConnectField::Domain => Some((self.domain.as_str(), self.domain_cursor, self.domain_selection)),
            _ => None,
        }
    }

    pub(crate) fn begin_mouse_selection(&mut self, field: QuickConnectField, column: usize) {
        self.drag_anchor = None;
        let mut anchor = None;
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let next_cursor = column.min(text_edit::char_len(text));
            *cursor = next_cursor;
            *selection = None;
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

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let next_cursor = column.min(text_edit::char_len(text));
            *cursor = next_cursor;
            *selection = if next_cursor == anchor { None } else { Some((anchor, next_cursor)) };
        }
    }

    pub(crate) fn mouse_drag_field(&self) -> Option<QuickConnectField> {
        self.drag_anchor.map(|(field, _)| field)
    }

    pub(crate) fn finish_mouse_selection(&mut self) {
        self.drag_anchor = None;
    }

    pub(crate) fn move_cursor_left(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = self.password_cursor.saturating_sub(1);
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            text_edit::clamp_cursor(text, cursor);
            let active_selection = text_edit::normalized_selection(text, *selection);
            *selection = None;
            if let Some((start, _)) = active_selection {
                *cursor = start;
            } else if *cursor > 0 {
                *cursor -= 1;
            }
        }
    }

    pub(crate) fn move_cursor_right(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = (self.password_cursor + 1).min(self.password.char_len());
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            text_edit::clamp_cursor(text, cursor);
            let len = text_edit::char_len(text);
            let active_selection = text_edit::normalized_selection(text, *selection);
            *selection = None;
            if let Some((_, end)) = active_selection {
                *cursor = end;
            } else if *cursor < len {
                *cursor += 1;
            }
        }
    }

    pub(crate) fn move_cursor_home(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = 0;
            return;
        }

        if let Some((_, cursor, selection)) = self.text_cursor_selection_mut(field) {
            *cursor = 0;
            *selection = None;
        }
    }

    pub(crate) fn move_cursor_end(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = self.password.char_len();
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            *cursor = text_edit::char_len(text);
            *selection = None;
        }
    }

    pub(crate) fn select_all(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = self.password.char_len();
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let len = text_edit::char_len(text);
            if len == 0 {
                *selection = None;
                *cursor = 0;
            } else {
                *selection = Some((0, len));
                *cursor = len;
            }
        }
    }

    fn delete_selection(&mut self, field: QuickConnectField) -> bool {
        let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) else {
            return false;
        };

        text_edit::delete_selection(text, cursor, selection)
    }

    pub(crate) fn insert_char(&mut self, field: QuickConnectField, ch: char) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password.insert_char(self.password_cursor, ch);
            self.password_cursor += 1;
            return;
        }

        let _ = self.delete_selection(field);
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            text_edit::clamp_cursor(text, cursor);
            let insert_at = text_edit::byte_index_for_char(text, *cursor);
            text.insert(insert_at, ch);
            *cursor += 1;
            *selection = None;
        }
    }

    pub(crate) fn backspace(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = self.password.backspace_char(self.password_cursor);
            return;
        }

        if self.delete_selection(field) {
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            text_edit::clamp_cursor(text, cursor);
            if *cursor == 0 {
                *selection = None;
                return;
            }

            let end = text_edit::byte_index_for_char(text, *cursor);
            let start = text_edit::byte_index_for_char(text, *cursor - 1);
            text.replace_range(start..end, "");
            *cursor -= 1;
            *selection = None;
        }
    }

    pub(crate) fn delete(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if field == QuickConnectField::Password {
            self.password_cursor = self.password.delete_char(self.password_cursor);
            return;
        }

        if self.delete_selection(field) {
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            text_edit::clamp_cursor(text, cursor);
            let len = text_edit::char_len(text);
            if *cursor >= len {
                *selection = None;
                return;
            }

            let start = text_edit::byte_index_for_char(text, *cursor);
            let end = text_edit::byte_index_for_char(text, *cursor + 1);
            text.replace_range(start..end, "");
            *selection = None;
        }
    }

    pub(crate) fn cursor_for_field(&self, field: QuickConnectField) -> Option<usize> {
        match field {
            QuickConnectField::Password => Some(self.password_cursor),
            _ => self.text_cursor_selection(field).map(|(_, cursor, _)| cursor),
        }
    }

    pub(crate) fn selection_for_field(&self, field: QuickConnectField) -> Option<(usize, usize)> {
        self.text_cursor_selection(field)
            .and_then(|(text, _, selection)| text_edit::normalized_selection(text, selection))
    }

    pub(crate) fn selected_profile_label(&self) -> &str {
        self.profile_options.get(self.profile_index).map(String::as_str).unwrap_or("default")
    }

    pub(crate) fn selected_profile_for_cli(&self) -> Option<String> {
        let profile = self.selected_profile_label();
        if profile.eq_ignore_ascii_case("default") {
            None
        } else {
            Some(profile.to_string())
        }
    }

    pub(crate) fn select_next_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.profile_options.len();
    }

    pub(crate) fn select_prev_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        if self.profile_index == 0 {
            self.profile_index = self.profile_options.len() - 1;
        } else {
            self.profile_index -= 1;
        }
    }

    pub(crate) fn build_submission(&self) -> Result<QuickConnectSubmission, QuickConnectValidationError> {
        let host_value = self.host.trim();
        if host_value.is_empty() {
            return Err(QuickConnectValidationError::HostRequired);
        }

        let user_value = self.user.trim();
        if self.is_rdp() && user_value.is_empty() {
            return Err(QuickConnectValidationError::RdpUserRequired);
        }

        let port = if self.is_rdp() {
            if self.port.trim().is_empty() {
                None
            } else {
                Some(self.port.trim().parse::<u16>().map_err(|_| QuickConnectValidationError::InvalidPort)?)
            }
        } else {
            None
        };

        let password = if self.is_rdp() {
            Some(
                self.password
                    .as_str()
                    .map_err(|err| QuickConnectValidationError::PasswordEncoding(err.to_string()))?,
            )
        } else {
            None
        };

        let target = if user_value.is_empty() {
            host_value.to_string()
        } else {
            format!("{}@{}", user_value, host_value)
        };

        let mut host = InventoryHost::new(target);
        host.protocol = self.protocol.clone();
        host.user = (!user_value.is_empty()).then(|| user_value.to_string());
        host.host = host_value.to_string();
        host.port = port;
        host.profile = if self.is_rdp() { None } else { self.selected_profile_for_cli() };
        if self.is_rdp() {
            host.rdp.domain = (!self.domain.trim().is_empty()).then(|| self.domain.trim().to_string());
        }

        Ok(QuickConnectSubmission {
            host,
            force_ssh_logging: !self.is_rdp() && self.ssh_logging,
            manual_rdp_password: password.filter(|value| !value.is_empty()).map(|value| SensitiveString::from(value.to_string())),
        })
    }
}

impl Drop for QuickConnectState {
    fn drop(&mut self) {
        self.password.clear();
    }
}

#[cfg(test)]
#[path = "../../test/tui/state/quick_connect.rs"]
mod tests;
