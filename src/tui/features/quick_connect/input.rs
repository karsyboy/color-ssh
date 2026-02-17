//! Quick-connect keyboard handling.

use crate::tui::{QuickConnectField, QuickConnectState, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl SessionManager {
    pub(crate) fn open_quick_connect_modal(&mut self) {
        let profiles = self.discover_quick_connect_profiles();
        self.quick_connect = Some(QuickConnectState::new(self.quick_connect_default_ssh_logging, profiles));
    }

    pub(crate) fn handle_quick_connect_key(&mut self, key: KeyEvent) {
        let mut should_submit = false;
        let mut should_close = false;

        if let Some(form) = self.quick_connect.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    should_close = true;
                }
                KeyCode::Tab | KeyCode::Down => {
                    form.selected = form.selected.next();
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.selected = form.selected.prev();
                }
                KeyCode::Enter => match form.selected {
                    QuickConnectField::Profile => {
                        form.error = None;
                        form.select_next_profile();
                    }
                    QuickConnectField::Logging => {
                        form.ssh_logging = !form.ssh_logging;
                    }
                    QuickConnectField::Connect => {
                        should_submit = true;
                    }
                    _ => {
                        form.selected = form.selected.next();
                    }
                },
                KeyCode::Char(' ') => {
                    if form.selected == QuickConnectField::Logging {
                        form.ssh_logging = !form.ssh_logging;
                    }
                }
                KeyCode::Left => {
                    if form.selected == QuickConnectField::Profile {
                        form.error = None;
                        form.select_prev_profile();
                    }
                }
                KeyCode::Right => {
                    if form.selected == QuickConnectField::Profile {
                        form.error = None;
                        form.select_next_profile();
                    }
                }
                KeyCode::Backspace => {
                    form.error = None;
                    match form.selected {
                        QuickConnectField::User => {
                            form.user.pop();
                        }
                        QuickConnectField::Host => {
                            form.host.pop();
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    form.error = None;
                    match form.selected {
                        QuickConnectField::User => form.user.push(ch),
                        QuickConnectField::Host => form.host.push(ch),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_quick_connect_modal();
        } else if should_close {
            self.quick_connect = None;
        }
    }

    pub(crate) fn submit_quick_connect_modal(&mut self) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        let user = form.user.trim().to_string();
        let host = form.host.trim().to_string();
        let profile = form.selected_profile_for_cli();
        let force_ssh_logging = form.ssh_logging;

        if host.is_empty() {
            form.error = Some("Host is required".to_string());
            form.selected = QuickConnectField::Host;
            return;
        }

        self.open_quick_connect_host(user, host, profile, force_ssh_logging);
    }
}
