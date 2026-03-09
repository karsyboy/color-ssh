//! Password vault unlock keyboard handling.

use crate::auth::secret::ExposeSecret;
use crate::auth::{agent, ipc::UnlockPolicy};
use crate::config;
use crate::inventory::ConnectionProtocol;
use crate::log_debug;
use crate::tui::{AppState, VaultStatusModalState, VaultUnlockAction, VaultUnlockState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const VAULT_UNLOCK_RETRY_NOTICE: &str = "Invalid master password. Try again.";
const MANUAL_VAULT_UNLOCK_RETRY_NOTICE: &str = "Vault unlock failed after multiple attempts. Try again.";

impl AppState {
    fn unlock_policy_for_action(&self, action: &VaultUnlockAction) -> UnlockPolicy {
        let auth_settings = match action {
            VaultUnlockAction::UnlockVault => config::auth_settings(),
            VaultUnlockAction::OpenHostTab { auth_settings, .. } | VaultUnlockAction::ReconnectTab { auth_settings, .. } => auth_settings.clone(),
        };

        UnlockPolicy::new(auth_settings.idle_timeout_seconds, auth_settings.session_timeout_seconds)
    }

    fn launch_protocol_for_vault_action<'a>(&'a self, action: &'a VaultUnlockAction) -> Option<&'a ConnectionProtocol> {
        match action {
            VaultUnlockAction::UnlockVault => None,
            VaultUnlockAction::OpenHostTab { host, .. } => Some(&host.protocol),
            VaultUnlockAction::ReconnectTab { tab_index, .. } => self.tabs.get(*tab_index).map(|tab| &tab.host.protocol),
        }
    }

    fn vault_unlock_fallback_notice(&self, action: &VaultUnlockAction, detail: impl Into<String>) -> String {
        let detail = detail.into();
        match self.launch_protocol_for_vault_action(action) {
            Some(ConnectionProtocol::Rdp) => format!("{detail}; continuing with the FreeRDP password prompt."),
            Some(ConnectionProtocol::Other(protocol)) => format!("{detail}; protocol '{}' is not supported for launch.", protocol),
            _ => format!("{detail}; continuing with the standard SSH password prompt."),
        }
    }

    pub(crate) fn open_vault_unlock(&mut self, entry_name: String, action: VaultUnlockAction) {
        log_debug!("Opening TUI password vault unlock prompt");
        self.quick_connect = None;
        self.vault_status_modal = None;
        self.vault_unlock = Some(VaultUnlockState::new(entry_name, action));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_manual_vault_unlock(&mut self) {
        log_debug!("Opening TUI password vault unlock prompt from host view");
        self.quick_connect = None;
        self.vault_status_modal = None;
        self.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_manual_vault_unlock_from_status(&mut self) {
        log_debug!("Opening TUI password vault unlock prompt from vault status modal");
        self.quick_connect = None;
        self.vault_status_modal = None;
        self.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault).return_to_vault_status());
        self.mark_ui_dirty();
    }

    fn restore_vault_status_modal(&mut self, message: Option<(String, bool)>) {
        self.open_vault_status_modal();
        if let Some((message, is_error)) = message {
            if let Some(modal) = self.vault_status_modal.as_mut() {
                modal.set_message(message, is_error);
            }
            self.mark_ui_dirty();
        }
    }

    fn close_manual_vault_unlock_after_attempt_limit(&mut self, return_to_vault_status: bool) {
        if return_to_vault_status {
            self.restore_vault_status_modal(Some((MANUAL_VAULT_UNLOCK_RETRY_NOTICE.to_string(), true)));
        } else {
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn open_vault_status_modal(&mut self) {
        log_debug!("Opening TUI vault status modal");
        self.quick_connect = None;
        self.vault_unlock = None;
        self.refresh_vault_status();
        self.vault_status_modal = Some(VaultStatusModalState::new());
        self.mark_ui_dirty();
    }

    fn apply_vault_status_modal_lock_result(&mut self, result: Result<crate::auth::ipc::VaultStatus, agent::AgentError>) {
        let fallback_vault_exists = self.vault_status.vault_exists;

        match result {
            Ok(status) => {
                self.set_vault_status(status);
                if let Some(modal) = self.vault_status_modal.as_mut() {
                    modal.set_message("Vault locked.".to_string(), false);
                }
            }
            Err(agent::AgentError::Io(_)) => {
                self.set_vault_status(crate::auth::ipc::VaultStatus::locked(fallback_vault_exists));
                if let Some(modal) = self.vault_status_modal.as_mut() {
                    modal.set_message("Vault already locked.".to_string(), false);
                }
            }
            Err(err) => {
                if let Some(modal) = self.vault_status_modal.as_mut() {
                    modal.set_message(format!("Failed to lock vault: {err}"), true);
                }
            }
        }
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_vault_status_modal_key(&mut self, key: KeyEvent) {
        if self.vault_status_modal.is_none() {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.vault_status_modal = None;
                self.mark_ui_dirty();
            }
            KeyCode::Char('v') if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                if self.vault_status.unlocked {
                    self.vault_status_modal = None;
                    self.mark_ui_dirty();
                } else {
                    self.open_manual_vault_unlock_from_status();
                }
            }
            KeyCode::Char('l') if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                if !self.vault_status.unlocked {
                    if let Some(modal) = self.vault_status_modal.as_mut() {
                        modal.set_message("Vault already locked.".to_string(), false);
                    }
                    self.mark_ui_dirty();
                    return;
                }

                let result = match agent::AgentClient::new() {
                    Ok(client) => client.lock(),
                    Err(err) => Err(err),
                };
                self.apply_vault_status_modal_lock_result(result);
            }
            _ => {}
        }
    }

    pub(crate) fn handle_vault_unlock_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.vault_unlock.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Esc => {
                let action = prompt.action.clone();
                let return_to_vault_status = prompt.return_to_vault_status;
                self.vault_unlock = None;
                if return_to_vault_status {
                    self.restore_vault_status_modal(None);
                    return;
                }
                let cancel_notice = (!action.is_manual_unlock()).then(|| self.vault_unlock_fallback_notice(&action, "Password vault unlock canceled"));
                self.complete_vault_unlock_action(action, None, cancel_notice);
            }
            KeyCode::Enter => {
                self.submit_vault_unlock();
            }
            KeyCode::Left => {
                prompt.move_cursor_left();
            }
            KeyCode::Right => {
                prompt.move_cursor_right();
            }
            KeyCode::Home => {
                prompt.move_cursor_home();
            }
            KeyCode::End => {
                prompt.move_cursor_end();
            }
            KeyCode::Backspace => {
                prompt.backspace();
                prompt.error = None;
            }
            KeyCode::Delete => {
                prompt.delete();
                prompt.error = None;
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.move_cursor_home();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.move_cursor_end();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                prompt.insert_char(ch);
                prompt.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn handle_vault_unlock_paste(&mut self, pasted: &str) {
        let Some(prompt) = self.vault_unlock.as_mut() else {
            return;
        };

        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        for ch in filtered.chars() {
            prompt.insert_char(ch);
        }
        prompt.error = None;
    }

    pub(crate) fn submit_vault_unlock(&mut self) {
        let Some(mut prompt) = self.vault_unlock.take() else {
            return;
        };

        let master_password = std::mem::take(&mut prompt.master_password);
        let action = prompt.action.clone();
        let entry_name = prompt.entry_name.clone();
        let return_to_vault_status = prompt.return_to_vault_status;
        let client = match agent::AgentClient::new() {
            Ok(client) => client,
            Err(err) => {
                if action.is_manual_unlock() {
                    prompt.error = Some(format!("Password vault agent could not be started ({err})."));
                    prompt.clear_master_password();
                    self.vault_unlock = Some(prompt);
                    return;
                }
                let fallback_notice = self.vault_unlock_fallback_notice(
                    &action,
                    format!("Password auto-login is unavailable because the password vault agent could not be started ({err})"),
                );
                self.complete_vault_unlock_action(action, None, Some(fallback_notice));
                return;
            }
        };

        let master_password = match master_password.into_sensitive_string() {
            Ok(master_password) => master_password,
            Err(err) => {
                if action.is_manual_unlock() {
                    prompt.error = Some(format!("Password vault input could not be processed ({err})."));
                    prompt.clear_master_password();
                    self.vault_unlock = Some(prompt);
                    return;
                }
                let fallback_notice = self.vault_unlock_fallback_notice(
                    &action,
                    format!("Password auto-login is unavailable because the password vault input could not be processed ({err})"),
                );
                self.complete_vault_unlock_action(action, None, Some(fallback_notice));
                return;
            }
        };
        let unlock_result = client.unlock(master_password.expose_secret(), self.unlock_policy_for_action(&action));

        match unlock_result {
            Ok(status) => {
                self.set_vault_status(status);
                log_debug!("TUI password vault unlock succeeded");
                if action.is_manual_unlock() && return_to_vault_status {
                    self.restore_vault_status_modal(None);
                    return;
                }
                let pass_entry_override = (!action.is_manual_unlock()).then_some(entry_name);
                self.complete_vault_unlock_action(action, pass_entry_override, None);
            }
            Err(agent::AgentError::InvalidMasterPassword) => {
                log_debug!("TUI password vault unlock failed due to invalid master password");
                prompt.attempts += 1;
                if prompt.attempts >= prompt.max_attempts {
                    if action.is_manual_unlock() {
                        self.close_manual_vault_unlock_after_attempt_limit(return_to_vault_status);
                        return;
                    }
                    let fallback_notice = self.vault_unlock_fallback_notice(
                        &action,
                        "Password auto-login is unavailable because vault unlock failed after multiple attempts",
                    );
                    self.complete_vault_unlock_action(action, None, Some(fallback_notice));
                    return;
                }

                prompt.error = Some(VAULT_UNLOCK_RETRY_NOTICE.to_string());
                prompt.clear_master_password();
                self.vault_unlock = Some(prompt);
            }
            Err(agent::AgentError::VaultNotInitialized) => {
                log_debug!("TUI password vault unlock failed because the vault is not initialized");
                if action.is_manual_unlock() {
                    prompt.error = Some("Password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first.".to_string());
                    prompt.clear_master_password();
                    self.vault_unlock = Some(prompt);
                    return;
                }
                let fallback_notice = self.vault_unlock_fallback_notice(
                    &action,
                    "Password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first.",
                );
                self.complete_vault_unlock_action(action, None, Some(fallback_notice));
            }
            Err(err) => {
                log_debug!("TUI password vault unlock failed: {}", err);
                if action.is_manual_unlock() {
                    prompt.error = Some(format!("Password vault could not be unlocked ({err})."));
                    prompt.clear_master_password();
                    self.vault_unlock = Some(prompt);
                    return;
                }
                let fallback_notice = self.vault_unlock_fallback_notice(
                    &action,
                    format!("Password auto-login is unavailable because the password vault could not be unlocked ({err})"),
                );
                self.complete_vault_unlock_action(action, None, Some(fallback_notice));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../test/tui/pass_prompt.rs"]
mod tests;
