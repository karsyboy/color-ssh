//! Launch-time RDP credential modal input handling.

use crate::log_error;
use crate::tui::features::terminal_session::launch::HostPassResolution;
use crate::tui::{
    AppState, RdpCredentialLaunchContext, RdpCredentialSubmission, RdpCredentialValidationError, RdpCredentialsAction, RdpCredentialsField, RdpCredentialsState,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(crate) fn open_rdp_credentials_modal(&mut self, host: &crate::inventory::InventoryHost, action: RdpCredentialsAction, notice: Option<String>) {
        self.quick_connect = None;
        self.vault_unlock = None;
        self.vault_status_modal = None;
        self.rdp_credentials = Some(RdpCredentialsState::new(host, action, notice));
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_rdp_credentials_key(&mut self, key: KeyEvent) {
        let mut should_submit = false;
        let mut should_close = false;

        if let Some(prompt) = self.rdp_credentials.as_mut() {
            prompt.finish_mouse_selection();
            match key.code {
                KeyCode::Esc => {
                    should_close = true;
                }
                KeyCode::Tab | KeyCode::Down => {
                    prompt.selected = prompt.selected.next();
                    prompt.error = None;
                }
                KeyCode::BackTab | KeyCode::Up => {
                    prompt.selected = prompt.selected.prev();
                    prompt.error = None;
                }
                KeyCode::Enter => {
                    should_submit = true;
                }
                KeyCode::Left => {
                    prompt.move_cursor_left(prompt.selected);
                }
                KeyCode::Right => {
                    prompt.move_cursor_right(prompt.selected);
                }
                KeyCode::Home => {
                    prompt.move_cursor_home(prompt.selected);
                }
                KeyCode::End => {
                    prompt.move_cursor_end(prompt.selected);
                }
                KeyCode::Backspace => {
                    prompt.backspace(prompt.selected);
                    prompt.error = None;
                }
                KeyCode::Delete => {
                    prompt.delete(prompt.selected);
                    prompt.error = None;
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    prompt.select_all(prompt.selected);
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    prompt.move_cursor_end(prompt.selected);
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    prompt.insert_char(prompt.selected, ch);
                    prompt.error = None;
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_rdp_credentials();
        } else if should_close {
            self.rdp_credentials = None;
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn handle_rdp_credentials_paste(&mut self, pasted: &str) {
        let Some(prompt) = self.rdp_credentials.as_mut() else {
            return;
        };

        prompt.finish_mouse_selection();
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        for ch in filtered.chars() {
            prompt.insert_char(prompt.selected, ch);
        }
        prompt.error = None;
    }

    pub(crate) fn submit_rdp_credentials(&mut self) {
        let Some(mut prompt) = self.rdp_credentials.take() else {
            return;
        };

        let action = prompt.action.clone();
        let Some(base_host) = self.base_host_for_rdp_credentials_action(&action) else {
            self.mark_ui_dirty();
            return;
        };

        let submission = match prompt.build_submission(&base_host) {
            Ok(submission) => submission,
            Err(err) => {
                prompt.error = Some(err.message());
                prompt.selected = match err {
                    RdpCredentialValidationError::UserRequired => RdpCredentialsField::User,
                    RdpCredentialValidationError::InvalidPort => RdpCredentialsField::Port,
                    RdpCredentialValidationError::PasswordEncoding(_) => RdpCredentialsField::Password,
                };
                self.rdp_credentials = Some(prompt);
                self.mark_ui_dirty();
                return;
            }
        };

        self.complete_rdp_credentials_action(action, submission);
    }

    fn base_host_for_rdp_credentials_action(&self, action: &RdpCredentialsAction) -> Option<crate::inventory::InventoryHost> {
        match action {
            RdpCredentialsAction::OpenHostTab { host, .. } => Some((**host).clone()),
            RdpCredentialsAction::ReconnectTab { tab_index, .. } => self.terminal_tab(*tab_index).map(|tab| tab.host.clone()),
        }
    }

    fn launch_context_for_rdp_credentials_action(action: &RdpCredentialsAction) -> RdpCredentialLaunchContext {
        match action {
            RdpCredentialsAction::OpenHostTab { launch_context, .. } | RdpCredentialsAction::ReconnectTab { launch_context, .. } => launch_context.clone(),
        }
    }

    fn complete_rdp_credentials_action(&mut self, action: RdpCredentialsAction, submission: RdpCredentialSubmission) {
        let RdpCredentialSubmission { host, manual_password } = submission;
        let launch_context = Self::launch_context_for_rdp_credentials_action(&action);
        let auth_resolution = if let Some(manual_rdp_password) = manual_password {
            HostPassResolution {
                pass_entry_override: None,
                pass_fallback_notice: None,
                disable_vault_autologin: true,
                manual_rdp_password: Some(manual_rdp_password),
            }
        } else {
            HostPassResolution {
                pass_entry_override: launch_context.pass_entry_override.clone(),
                pass_fallback_notice: launch_context.pass_fallback_notice.clone(),
                disable_vault_autologin: launch_context.disable_vault_autologin,
                manual_rdp_password: None,
            }
        };

        match action {
            RdpCredentialsAction::OpenHostTab { force_ssh_logging, .. } => match Self::resolve_session_profile(&host) {
                Ok(session_profile) => self.open_host_tab_with_auth(host, force_ssh_logging, auth_resolution, session_profile),
                Err(err) => self.open_host_tab_error(host, force_ssh_logging, err.to_string()),
            },
            RdpCredentialsAction::ReconnectTab { tab_index, .. } => match Self::resolve_session_profile(&host) {
                Ok(session_profile) => self.reconnect_session_with_auth(tab_index, host, auth_resolution, session_profile),
                Err(err) => {
                    let err_message = err.to_string();
                    log_error!("Failed to prepare RDP reconnect: {}", err_message);
                    if let Some(tab) = self.terminal_tab_mut(tab_index) {
                        tab.session = None;
                        tab.session_error = Some(err_message);
                    }
                }
            },
        }
    }
}
