//! Password vault unlock modal state and deferred action context.

use crate::auth::secret::SensitiveBuffer;
use crate::inventory::InventoryHost;

pub(crate) const VAULT_UNLOCK_MAX_ATTEMPTS: usize = 3;

pub(crate) struct VaultStatusModalState {
    pub(crate) message: Option<String>,
    pub(crate) message_is_error: bool,
}

impl VaultStatusModalState {
    pub(crate) fn new() -> Self {
        Self {
            message: None,
            message_is_error: false,
        }
    }

    pub(crate) fn set_message(&mut self, message: String, is_error: bool) {
        self.message = Some(message);
        self.message_is_error = is_error;
    }
}

#[derive(Debug, Clone)]
pub(crate) enum VaultUnlockAction {
    UnlockVault,
    OpenHostTab { host: Box<InventoryHost>, force_ssh_logging: bool },
    ReconnectTab { tab_index: usize },
}

impl VaultUnlockAction {
    pub(crate) fn is_manual_unlock(&self) -> bool {
        matches!(self, Self::UnlockVault)
    }

    pub(crate) fn prompt_target_label(&self) -> &'static str {
        if self.is_manual_unlock() { "Vault: " } else { "Entry: " }
    }

    pub(crate) fn prompt_target_value<'a>(&self, entry_name: &'a str) -> &'a str {
        if self.is_manual_unlock() { "Shared session" } else { entry_name }
    }

    pub(crate) fn prompt_submit_hint(&self) -> &'static str {
        "[Enter] Unlock"
    }

    pub(crate) fn prompt_cancel_hint(&self) -> &'static str {
        if self.is_manual_unlock() {
            "[Esc] Cancel"
        } else {
            "[Esc] Continue without auto-login"
        }
    }
}

pub(crate) struct VaultUnlockState {
    pub(crate) entry_name: String,
    pub(crate) master_password: SensitiveBuffer,
    pub(crate) cursor: usize,
    pub(crate) attempts: usize,
    pub(crate) max_attempts: usize,
    pub(crate) error: Option<String>,
    pub(crate) action: VaultUnlockAction,
    pub(crate) return_to_vault_status: bool,
}

impl VaultUnlockState {
    pub(crate) fn new(entry_name: String, action: VaultUnlockAction) -> Self {
        Self {
            entry_name,
            master_password: SensitiveBuffer::new(),
            cursor: 0,
            attempts: 0,
            max_attempts: VAULT_UNLOCK_MAX_ATTEMPTS,
            error: None,
            action,
            return_to_vault_status: false,
        }
    }

    pub(crate) fn return_to_vault_status(mut self) -> Self {
        self.return_to_vault_status = true;
        self
    }

    pub(crate) fn masked_master_password(&self) -> String {
        self.master_password.masked()
    }

    pub(crate) fn remaining_attempts(&self) -> usize {
        self.max_attempts.saturating_sub(self.attempts)
    }

    pub(crate) fn clear_master_password(&mut self) {
        self.master_password.clear();
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.master_password.char_len());
    }

    pub(crate) fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_end(&mut self) {
        self.cursor = self.master_password.char_len();
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        self.master_password.insert_char(self.cursor, ch);
        self.cursor += 1;
    }

    pub(crate) fn backspace(&mut self) {
        self.cursor = self.master_password.backspace_char(self.cursor);
    }

    pub(crate) fn delete(&mut self) {
        self.cursor = self.master_password.delete_char(self.cursor);
    }
}

impl Drop for VaultUnlockState {
    fn drop(&mut self) {
        self.master_password.clear();
    }
}
