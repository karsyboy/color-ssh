//! Password vault unlock keyboard handling.

use crate::auth::{agent, ipc::UnlockPolicy};
use crate::config;
use crate::tui::{SessionManager, VaultUnlockAction, VaultUnlockState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroize;

const VAULT_UNLOCK_CANCEL_NOTICE: &str = "Password vault unlock canceled; falling back to the standard SSH password prompt.";
const VAULT_UNLOCK_RETRY_NOTICE: &str = "Invalid master password. Try again.";

fn current_unlock_policy() -> UnlockPolicy {
    let auth_settings = config::auth_settings();
    UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
}

impl SessionManager {
    pub(crate) fn open_vault_unlock(&mut self, entry_name: String, action: VaultUnlockAction) {
        self.quick_connect = None;
        self.vault_unlock = Some(VaultUnlockState::new(entry_name, action));
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_vault_unlock_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.vault_unlock.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Esc => {
                let action = prompt.action.clone();
                self.vault_unlock = None;
                self.complete_vault_unlock_action(action, None, Some(VAULT_UNLOCK_CANCEL_NOTICE.to_string()));
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

        let mut master_password = std::mem::take(&mut prompt.master_password);
        let action = prompt.action.clone();
        let entry_name = prompt.entry_name.clone();
        let client = match agent::AgentClient::new() {
            Ok(client) => client,
            Err(err) => {
                master_password.zeroize();
                self.complete_vault_unlock_action(
                    action,
                    None,
                    Some(format!(
                        "Password auto-login is unavailable because the password vault agent could not be started ({err}); continuing with the standard SSH password prompt."
                    )),
                );
                return;
            }
        };

        let unlock_result = client.unlock(&master_password, current_unlock_policy());
        master_password.zeroize();

        match unlock_result {
            Ok(_) => match client.get_secret(&entry_name) {
                Ok(password) => {
                    self.complete_vault_unlock_action(action, Some(password), None);
                }
                Err(agent::AgentError::EntryNotFound) => {
                    self.complete_vault_unlock_action(
                        action,
                        None,
                        Some(format!(
                            "Password auto-login is unavailable because vault entry '{}' was not found; continuing with the standard SSH password prompt.",
                            entry_name
                        )),
                    );
                }
                Err(err) => {
                    self.complete_vault_unlock_action(
                        action,
                        None,
                        Some(format!(
                            "Password auto-login is unavailable because the password vault could not provide entry '{}' ({err}); continuing with the standard SSH password prompt.",
                            entry_name
                        )),
                    );
                }
            },
            Err(agent::AgentError::InvalidMasterPassword) => {
                prompt.attempts += 1;
                if prompt.attempts >= prompt.max_attempts {
                    self.complete_vault_unlock_action(
                        action,
                        None,
                        Some("Password auto-login is unavailable because vault unlock failed after multiple attempts; continuing with the standard SSH password prompt.".to_string()),
                    );
                    return;
                }

                prompt.error = Some(VAULT_UNLOCK_RETRY_NOTICE.to_string());
                prompt.clear_master_password();
                self.vault_unlock = Some(prompt);
            }
            Err(err) => {
                self.complete_vault_unlock_action(
                    action,
                    None,
                    Some(format!(
                        "Password auto-login is unavailable because the password vault could not be unlocked ({err}); continuing with the standard SSH password prompt."
                    )),
                );
            }
        }
    }
}
