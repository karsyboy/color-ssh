//! Launch-time RDP credential modal state.

use crate::auth::secret::{SensitiveBuffer, SensitiveString};
use crate::inventory::InventoryHost;
use crate::tui::text_edit;

type TextSelection = Option<(usize, usize)>;
type TextCursorSelectionMut<'a> = (&'a mut String, &'a mut usize, &'a mut TextSelection);
type TextCursorSelection<'a> = (&'a str, usize, TextSelection);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RdpCredentialsField {
    User,
    Domain,
    Port,
    Password,
}

impl RdpCredentialsField {
    pub(crate) fn next(self) -> Self {
        match self {
            Self::User => Self::Domain,
            Self::Domain => Self::Port,
            Self::Port => Self::Password,
            Self::Password => Self::User,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::User => Self::Password,
            Self::Domain => Self::User,
            Self::Port => Self::Domain,
            Self::Password => Self::Port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RdpCredentialLaunchContext {
    pub(crate) pass_entry_override: Option<String>,
    pub(crate) pass_fallback_notice: Option<String>,
    pub(crate) disable_vault_autologin: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum RdpCredentialsAction {
    OpenHostTab {
        host: Box<InventoryHost>,
        force_ssh_logging: bool,
        launch_context: RdpCredentialLaunchContext,
    },
    ReconnectTab {
        tab_index: usize,
        launch_context: RdpCredentialLaunchContext,
    },
}

#[derive(Debug)]
pub(crate) struct RdpCredentialSubmission {
    pub(crate) host: InventoryHost,
    pub(crate) manual_password: Option<SensitiveString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RdpCredentialValidationError {
    UserRequired,
    InvalidPort,
    PasswordEncoding(String),
}

impl RdpCredentialValidationError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::UserRequired => "RDP username is required.".to_string(),
            Self::InvalidPort => "RDP port must be a number between 1 and 65535.".to_string(),
            Self::PasswordEncoding(detail) => format!("RDP password could not be processed ({detail})."),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RdpCredentialsState {
    pub(crate) target_label: String,
    pub(crate) user: String,
    pub(crate) user_cursor: usize,
    pub(crate) user_selection: Option<(usize, usize)>,
    pub(crate) domain: String,
    pub(crate) domain_cursor: usize,
    pub(crate) domain_selection: Option<(usize, usize)>,
    pub(crate) port: String,
    pub(crate) port_cursor: usize,
    pub(crate) port_selection: Option<(usize, usize)>,
    pub(crate) password: SensitiveBuffer,
    pub(crate) password_cursor: usize,
    drag_anchor: Option<(RdpCredentialsField, usize)>,
    pub(crate) selected: RdpCredentialsField,
    pub(crate) notice: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) action: RdpCredentialsAction,
}

impl RdpCredentialsState {
    pub(crate) fn new(host: &InventoryHost, action: RdpCredentialsAction, notice: Option<String>) -> Self {
        let target_label = if host.name == host.host {
            host.name.clone()
        } else {
            format!("{} ({})", host.name, host.host)
        };
        let user = host.user.clone().unwrap_or_default();
        let domain = host.rdp.domain.clone().unwrap_or_default();
        let port = host.port.map(|value| value.to_string()).unwrap_or_default();
        let selected = if user.trim().is_empty() {
            RdpCredentialsField::User
        } else {
            RdpCredentialsField::Password
        };

        Self {
            target_label,
            user_cursor: text_edit::char_len(&user),
            user_selection: None,
            user,
            domain_cursor: text_edit::char_len(&domain),
            domain_selection: None,
            domain,
            port_cursor: text_edit::char_len(&port),
            port_selection: None,
            port,
            password: SensitiveBuffer::new(),
            password_cursor: 0,
            drag_anchor: None,
            selected,
            notice,
            error: None,
            action,
        }
    }

    fn text_field_mut(&mut self, field: RdpCredentialsField) -> Option<TextCursorSelectionMut<'_>> {
        match field {
            RdpCredentialsField::User => Some((&mut self.user, &mut self.user_cursor, &mut self.user_selection)),
            RdpCredentialsField::Domain => Some((&mut self.domain, &mut self.domain_cursor, &mut self.domain_selection)),
            RdpCredentialsField::Port => Some((&mut self.port, &mut self.port_cursor, &mut self.port_selection)),
            RdpCredentialsField::Password => None,
        }
    }

    fn text_field(&self, field: RdpCredentialsField) -> Option<TextCursorSelection<'_>> {
        match field {
            RdpCredentialsField::User => Some((self.user.as_str(), self.user_cursor, self.user_selection)),
            RdpCredentialsField::Domain => Some((self.domain.as_str(), self.domain_cursor, self.domain_selection)),
            RdpCredentialsField::Port => Some((self.port.as_str(), self.port_cursor, self.port_selection)),
            RdpCredentialsField::Password => None,
        }
    }

    pub(crate) fn cursor_for_field(&self, field: RdpCredentialsField) -> usize {
        match field {
            RdpCredentialsField::User | RdpCredentialsField::Domain | RdpCredentialsField::Port => {
                self.text_field(field).map(|(_, cursor, _)| cursor).unwrap_or(0)
            }
            RdpCredentialsField::Password => self.password_cursor,
        }
    }

    pub(crate) fn text_for_field(&self, field: RdpCredentialsField) -> Option<&str> {
        match field {
            RdpCredentialsField::User => Some(self.user.as_str()),
            RdpCredentialsField::Domain => Some(self.domain.as_str()),
            RdpCredentialsField::Port => Some(self.port.as_str()),
            RdpCredentialsField::Password => None,
        }
    }

    pub(crate) fn selection_for_field(&self, field: RdpCredentialsField) -> Option<(usize, usize)> {
        self.text_field(field)
            .and_then(|(text, _, selection)| text_edit::normalized_selection(text, selection))
    }

    pub(crate) fn masked_password(&self) -> String {
        self.password.masked()
    }

    pub(crate) fn begin_mouse_selection(&mut self, field: RdpCredentialsField, column: usize) {
        self.drag_anchor = None;
        let mut anchor = None;
        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
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

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            let next_cursor = column.min(text_edit::char_len(text));
            *cursor = next_cursor;
            *selection = if next_cursor == anchor { None } else { Some((anchor, next_cursor)) };
        }
    }

    pub(crate) fn mouse_drag_field(&self) -> Option<RdpCredentialsField> {
        self.drag_anchor.map(|(field, _)| field)
    }

    pub(crate) fn finish_mouse_selection(&mut self) {
        self.drag_anchor = None;
    }

    pub(crate) fn move_cursor_left(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = self.password_cursor.saturating_sub(1);
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::move_cursor_left(text, cursor, selection);
        }
    }

    pub(crate) fn move_cursor_right(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = (self.password_cursor + 1).min(self.password.char_len());
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::move_cursor_right(text, cursor, selection);
        }
    }

    pub(crate) fn move_cursor_home(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = 0;
            return;
        }

        if let Some((_, cursor, selection)) = self.text_field_mut(field) {
            text_edit::move_cursor_home(cursor, selection);
        }
    }

    pub(crate) fn move_cursor_end(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = self.password.char_len();
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::move_cursor_end(text, cursor, selection);
        }
    }

    pub(crate) fn select_all(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = self.password.char_len();
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::select_all(text, cursor, selection);
        }
    }

    pub(crate) fn insert_char(&mut self, field: RdpCredentialsField, ch: char) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password.insert_char(self.password_cursor, ch);
            self.password_cursor += 1;
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::insert_char(text, cursor, selection, ch);
        }
    }

    pub(crate) fn backspace(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = self.password.backspace_char(self.password_cursor);
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::backspace(text, cursor, selection);
        }
    }

    pub(crate) fn delete(&mut self, field: RdpCredentialsField) {
        self.finish_mouse_selection();
        if field == RdpCredentialsField::Password {
            self.password_cursor = self.password.delete_char(self.password_cursor);
            return;
        }

        if let Some((text, cursor, selection)) = self.text_field_mut(field) {
            text_edit::delete_char(text, cursor, selection);
        }
    }

    pub(crate) fn build_submission(&self, base_host: &InventoryHost) -> Result<RdpCredentialSubmission, RdpCredentialValidationError> {
        let user = self.user.trim();
        if user.is_empty() {
            return Err(RdpCredentialValidationError::UserRequired);
        }

        let port = if self.port.trim().is_empty() {
            None
        } else {
            Some(self.port.trim().parse::<u16>().map_err(|_| RdpCredentialValidationError::InvalidPort)?)
        };

        let password = self
            .password
            .as_str()
            .map_err(|err| RdpCredentialValidationError::PasswordEncoding(err.to_string()))?;

        let mut host = base_host.clone();
        host.user = Some(user.to_string());
        host.rdp.domain = (!self.domain.trim().is_empty()).then(|| self.domain.trim().to_string());
        host.port = port;

        Ok(RdpCredentialSubmission {
            host,
            manual_password: (!password.is_empty()).then(|| SensitiveString::from(password.to_string())),
        })
    }
}

impl Drop for RdpCredentialsState {
    fn drop(&mut self) {
        self.password.clear();
    }
}

#[cfg(test)]
#[path = "../../test/tui/state/rdp_prompt.rs"]
mod tests;
