//! Quick-connect keyboard handling.

use crate::tui::{QuickConnectField, QuickConnectState, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl SessionManager {
    // Modal lifecycle.
    pub(crate) fn open_quick_connect_modal(&mut self) {
        let profiles = self.discover_quick_connect_profiles();
        self.quick_connect = Some(QuickConnectState::new(self.quick_connect_default_ssh_logging, profiles));
    }

    // Keyboard handling inside quick-connect modal.
    pub(crate) fn handle_quick_connect_key(&mut self, key: KeyEvent) {
        let mut should_submit = false;
        let mut should_close = false;

        if let Some(form) = self.quick_connect.as_mut() {
            form.finish_mouse_selection();
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
                        form.select_next_profile();
                    }
                    QuickConnectField::Logging => {
                        form.ssh_logging = !form.ssh_logging;
                    }
                    QuickConnectField::Connect => {
                        should_submit = true;
                    }
                    QuickConnectField::Cancel => {
                        should_close = true;
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
                KeyCode::Left => match form.selected {
                    QuickConnectField::Profile => form.select_prev_profile(),
                    QuickConnectField::User | QuickConnectField::Host => form.move_cursor_left(form.selected),
                    _ => {}
                },
                KeyCode::Right => match form.selected {
                    QuickConnectField::Profile => form.select_next_profile(),
                    QuickConnectField::User | QuickConnectField::Host => form.move_cursor_right(form.selected),
                    _ => {}
                },
                KeyCode::Home => {
                    form.move_cursor_home(form.selected);
                }
                KeyCode::End => {
                    form.move_cursor_end(form.selected);
                }
                KeyCode::Backspace => {
                    form.backspace(form.selected);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                }
                KeyCode::Delete => {
                    form.delete(form.selected);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    form.insert_char(form.selected, ch);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.select_all(form.selected);
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.move_cursor_end(form.selected);
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

    // Form submit validation + action.
    pub(crate) fn submit_quick_connect_modal(&mut self) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        let user = form.user.trim().to_string();
        let host = form.host.trim().to_string();
        let profile = form.selected_profile_for_cli();
        let force_ssh_logging = form.ssh_logging;

        if host.is_empty() {
            form.host_required = true;
            form.selected = QuickConnectField::Host;
            return;
        }

        form.host_required = false;
        self.open_quick_connect_host(user, host, profile, force_ssh_logging);
    }
}
