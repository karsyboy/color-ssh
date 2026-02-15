//! Keyboard and mouse input handling

use super::App;
use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use std::io::{self, Write};
use std::time::Instant;

impl App {
    /// Handle keyboard input
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        // Only process key press events, not release
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        if self.search_mode {
            return self.handle_search_key(key);
        }

        // If focused on a tab with an active session, forward most keys to PTY
        if !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            return self.handle_tab_key(key);
        }

        self.handle_manager_key(key)
    }

    /// Handle keys while in search mode
    fn handle_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.update_filtered_hosts();
            }
            KeyCode::Enter => {
                self.search_mode = false;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_filtered_hosts();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.update_filtered_hosts();
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys while in terminal search mode
    fn handle_terminal_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.terminal_search_mode = false;
                self.terminal_search_query.clear();
                self.terminal_search_matches.clear();
                self.terminal_search_current = 0;
            }
            KeyCode::Enter | KeyCode::Down => {
                // Next match
                if !self.terminal_search_matches.is_empty() {
                    self.terminal_search_current = (self.terminal_search_current + 1) % self.terminal_search_matches.len();
                    self.scroll_to_search_match();
                }
            }
            KeyCode::Up => {
                // Previous match
                if !self.terminal_search_matches.is_empty() {
                    if self.terminal_search_current == 0 {
                        self.terminal_search_current = self.terminal_search_matches.len() - 1;
                    } else {
                        self.terminal_search_current -= 1;
                    }
                    self.scroll_to_search_match();
                }
            }
            KeyCode::Backspace => {
                self.terminal_search_query.pop();
                self.update_terminal_search();
            }
            KeyCode::Char(c) => {
                self.terminal_search_query.push(c);
                self.update_terminal_search();
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle keys when focused on a tab with an active session
    fn handle_tab_key(&mut self, key: KeyEvent) -> io::Result<()> {
        // If in terminal search mode, handle search keys
        if self.terminal_search_mode {
            return self.handle_terminal_search_key(key);
        }

        match key.code {
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+B: Toggle host panel visibility
                self.host_panel_visible = !self.host_panel_visible;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::ALT => {
                // Alt+c: copy selection to clipboard
                if self.selection_start.is_some() && self.selection_end.is_some() {
                    self.copy_selection_to_clipboard();
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Close current tab
                self.tabs.remove(self.selected_tab);
                if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
                    self.selected_tab -= 1;
                }
                if self.tabs.is_empty() {
                    self.focus_on_manager = true;
                }
            }
            KeyCode::Tab if key.modifiers.is_empty() => {
                // Tab without modifiers - forward to PTY for command completion
                self.send_key_to_pty(key)?;
            }
            KeyCode::BackTab => {
                // Shift+Tab: Switch focus back to manager (show panel if hidden)
                self.focus_on_manager = true;
                if !self.host_panel_visible {
                    self.host_panel_visible = true;
                }
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                // Alt+Left: previous tab
                if self.selected_tab > 0 {
                    self.selected_tab -= 1;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.ensure_tab_visible();
                }
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                // Alt+Right: next tab
                if self.selected_tab < self.tabs.len() - 1 {
                    self.selected_tab += 1;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.ensure_tab_visible();
                }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+F: Terminal search (only if no TUI app is running)
                if !self.is_pty_mouse_mode_active() {
                    self.terminal_search_mode = true;
                    self.terminal_search_query.clear();
                    self.terminal_search_matches.clear();
                    self.terminal_search_current = 0;
                } else {
                    // Forward to PTY if a TUI app is active
                    self.send_key_to_pty(key)?;
                }
            }
            KeyCode::PageUp if key.modifiers.contains(KeyModifiers::SHIFT) => {
                // Shift+PageUp: scroll up in scrollback
                let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                let tab = &mut self.tabs[self.selected_tab];
                tab.scroll_offset = tab.scroll_offset.saturating_add(10).min(max_scrollback);
            }
            KeyCode::PageDown if key.modifiers.contains(KeyModifiers::SHIFT) => {
                // Shift+PageDown: scroll down (towards live)
                let tab = &mut self.tabs[self.selected_tab];
                tab.scroll_offset = tab.scroll_offset.saturating_sub(10);
            }
            KeyCode::Enter => {
                // If session is disconnected, reconnect
                let tab = &self.tabs[self.selected_tab];
                let is_exited = if let Some(session) = &tab.session {
                    *session.exited.lock().unwrap()
                } else {
                    true
                };
                if is_exited {
                    self.reconnect_session();
                } else {
                    // Forward Enter to PTY
                    self.tabs[self.selected_tab].scroll_offset = 0;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.send_key_to_pty(key)?;
                }
            }
            _ => {
                // Any other key: reset scroll to live view, clear selection, and forward to PTY
                self.tabs[self.selected_tab].scroll_offset = 0;
                self.selection_start = None;
                self.selection_end = None;
                self.send_key_to_pty(key)?;
            }
        }
        Ok(())
    }

    /// Handle keys when focused on the session manager host list
    fn handle_manager_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            // Global commands
            KeyCode::Esc => {
                // If on tabs, go back to manager (show panel if hidden)
                if !self.focus_on_manager {
                    self.focus_on_manager = true;
                    if !self.host_panel_visible {
                        self.host_panel_visible = true;
                    }
                } else {
                    self.should_exit = true;
                }
            }

            // Tab management
            KeyCode::BackTab => {
                // Shift+Tab: Switch focus between manager and tabs (show panel if switching to manager)
                if !self.tabs.is_empty() {
                    self.focus_on_manager = !self.focus_on_manager;
                    if self.focus_on_manager && !self.host_panel_visible {
                        self.host_panel_visible = true;
                    }
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Close current tab
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    self.tabs.remove(self.selected_tab);
                    if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
                        self.selected_tab -= 1;
                    }
                    if self.tabs.is_empty() {
                        self.focus_on_manager = true;
                    }
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Connect to the tab's host (exit session manager and connect normally)
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    let host = self.tabs[self.selected_tab].host.clone();
                    self.selected_host_to_connect = Some(host);
                    self.should_exit = true;
                }
            }

            // Tab navigation (when focused on tabs)
            KeyCode::Left if !self.focus_on_manager => {
                if self.selected_tab > 0 {
                    self.selected_tab -= 1;
                }
            }
            KeyCode::Right if !self.focus_on_manager => {
                if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() - 1 {
                    self.selected_tab += 1;
                }
            }

            // Host list navigation (when focused on manager)
            KeyCode::Char('f') if self.focus_on_manager && key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_mode = true;
            }
            KeyCode::Left if self.focus_on_manager && self.host_panel_visible && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Left: shrink host panel
                self.host_panel_width = self.host_panel_width.saturating_sub(5).max(15);
            }
            KeyCode::Right if self.focus_on_manager && self.host_panel_visible && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Right: grow host panel
                self.host_panel_width = (self.host_panel_width + 5).min(80);
            }
            KeyCode::Up if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() && self.selected_host > 0 {
                    self.selected_host -= 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Down if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() && self.selected_host < self.filtered_hosts.len() - 1 {
                    self.selected_host += 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageUp if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = self.selected_host.saturating_sub(page_size);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageDown if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = (self.selected_host + page_size).min(self.filtered_hosts.len().saturating_sub(1));
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Home if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = 0;
                    self.host_list_state.select(Some(0));
                }
            }
            KeyCode::End if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = self.filtered_hosts.len().saturating_sub(1);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Enter if self.focus_on_manager => {
                self.select_host_to_connect();
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle mouse events
    pub(super) fn handle_mouse(&mut self, mouse: event::MouseEvent) -> io::Result<()> {
        // If terminal search mode is active but PTY wants mouse events (TUI app running),
        // exit search mode and forward mouse to PTY
        if self.terminal_search_mode && self.is_pty_mouse_mode_active() {
            self.terminal_search_mode = false;
            self.terminal_search_query.clear();
            self.terminal_search_matches.clear();
            self.terminal_search_current = 0;
        }
        
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if click is on the exit button
                let exit_area = self.exit_button_area;
                if exit_area.width > 0
                    && mouse.column >= exit_area.x
                    && mouse.column < exit_area.x + exit_area.width
                    && mouse.row >= exit_area.y
                    && mouse.row < exit_area.y + exit_area.height
                {
                    self.should_exit = true;
                    return Ok(());
                }

                // Check if click is on the divider between host panel and terminal panel
                let divider_col = self.host_panel_area.x + self.host_panel_area.width;
                if self.host_panel_visible && self.host_panel_area.width > 0 && mouse.column == divider_col {
                    self.is_dragging_divider = true;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                    return Ok(());
                }

                // Check if click is in the host list area (select specific host)
                let host_area = self.host_list_area;
                if self.host_panel_visible
                    && host_area.width > 0
                    && host_area.height > 2
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row > host_area.y
                    && mouse.row < host_area.y + host_area.height.saturating_sub(1)
                {
                    // Calculate which host was clicked (accounting for border and scroll)
                    let clicked_row = (mouse.row - host_area.y - 1) as usize;
                    let clicked_index = self.host_scroll_offset + clicked_row;
                    if clicked_index < self.filtered_hosts.len() {
                        // Check for double-click (same position within 400ms)
                        let now = Instant::now();
                        let is_double_click = if let Some((last_time, last_col, last_row)) = self.last_click {
                            now.duration_since(last_time).as_millis() < 400 && last_col == mouse.column && last_row == mouse.row
                        } else {
                            false
                        };

                        self.selected_host = clicked_index;
                        self.host_list_state.select(Some(self.selected_host));
                        self.focus_on_manager = true;

                        if is_double_click {
                            // Double-click: open host in a new tab
                            self.last_click = None;
                            self.select_host_to_connect();
                        } else {
                            self.last_click = Some((now, mouse.column, mouse.row));
                        }
                    } else {
                        // Clicked in host list area but past the last host — just focus
                        self.focus_on_manager = true;
                    }
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                    return Ok(());
                }

                // Check if click is anywhere in the host panel (list + info) to focus it
                let panel_area = self.host_panel_area;
                if self.host_panel_visible
                    && panel_area.width > 0
                    && panel_area.height > 0
                    && mouse.column >= panel_area.x
                    && mouse.column < panel_area.x + panel_area.width
                    && mouse.row >= panel_area.y
                    && mouse.row < panel_area.y + panel_area.height
                {
                    self.focus_on_manager = true;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                    return Ok(());
                }

                // Check if click is in the tab bar area
                let tab_area = self.tab_bar_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && tab_area.height > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    // Calculate which tab was clicked based on tab title widths
                    // Tab format: " [title ×] " (selected) or " title × " (unselected)
                    // Widths: prefix(1-2) + title + space(1) + ×(1) + suffix(1-2) = title.len() + 5
                    let visual_col = (mouse.column - tab_area.x).saturating_sub(1) as usize; // subtract left border
                    // When scrolled, the ◀ indicator takes 1 column at the start
                    let indicator_offset: usize = if self.tab_scroll_offset > 0 { 1 } else { 0 };
                    let click_col = if visual_col < indicator_offset {
                        // Clicked on the ◀ indicator itself — don't match any tab
                        usize::MAX
                    } else {
                        visual_col - indicator_offset + self.tab_scroll_offset
                    };
                    let mut cumulative_width: usize = 0;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let is_selected = idx == self.selected_tab && !self.focus_on_manager;
                        // Selected: " [title ×] " = 2 + title + 1 + 1 + 2 = title.len() + 6
                        // Unselected: " title × " = 1 + title + 1 + 1 + 1 = title.len() + 4
                        let tab_width = if is_selected { tab.title.len() + 6 } else { tab.title.len() + 4 };
                        let prefix_len: usize = if is_selected { 2 } else { 1 };
                        let separator_width = if idx < self.tabs.len() - 1 { 1 } else { 0 }; // "│"
                        if click_col < cumulative_width + tab_width {
                            // Check if click is on the × close button
                            // × is at position: cumulative_width + prefix_len + title.len() + 1 (the space)
                            let close_pos = cumulative_width + prefix_len + tab.title.len() + 1;
                            if click_col == close_pos {
                                // Close this tab
                                self.tabs.remove(idx);
                                if self.tabs.is_empty() {
                                    self.selected_tab = 0;
                                    self.focus_on_manager = true;
                                } else if self.selected_tab >= self.tabs.len() {
                                    self.selected_tab = self.tabs.len() - 1;
                                }
                            } else {
                                // Select this tab
                                self.selected_tab = idx;
                                self.focus_on_manager = false;
                            }
                            self.ensure_tab_visible();
                            self.selection_start = None;
                            self.selection_end = None;
                            self.is_selecting = false;
                            return Ok(());
                        }
                        cumulative_width += tab_width + separator_width;
                    }
                    // Clicked in tab bar but past all tab labels — still focus on tabs
                    self.focus_on_manager = false;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                    return Ok(());
                }

                // Check if click is in tab content area
                let area = self.tab_content_area;
                if !self.tabs.is_empty()
                    && self.selected_tab < self.tabs.len()
                    && area.width > 2
                    && area.height > 2
                    && mouse.column > area.x
                    && mouse.column < area.x + area.width - 1
                    && mouse.row > area.y
                    && mouse.row < area.y + area.height - 1
                {
                    self.focus_on_manager = false;
                    let alt_held = mouse.modifiers.contains(KeyModifiers::ALT);

                    if self.is_pty_mouse_mode_active() && !alt_held {
                        // PTY wants mouse events — forward click to the TUI app
                        // Also reset scroll to live view if scrolled back
                        if self.tabs[self.selected_tab].scroll_offset > 0 {
                            self.tabs[self.selected_tab].scroll_offset = 0;
                        }
                        self.selection_start = None;
                        self.selection_end = None;
                        self.is_selecting = false;
                        self.selection_dragged = false;
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(0, col, row, false)?;
                        }
                    } else if !self.is_pty_mouse_mode_active() || alt_held {
                        // Only start text selection if PTY doesn't want mouse events (or Alt is held to force selection)
                        let vt_row = mouse.row.saturating_sub(area.y + 1);
                        let vt_col = mouse.column.saturating_sub(area.x + 1);
                        let scroll_offset = self.tabs[self.selected_tab].scroll_offset;
                        let abs_row = vt_row as i64 - scroll_offset as i64;
                        self.selection_start = Some((abs_row, vt_col));
                        self.selection_end = Some((abs_row, vt_col));
                        self.is_selecting = true;
                        self.selection_dragged = false;
                    }
                } else if !self.tabs.is_empty()
                    && self.selected_tab < self.tabs.len()
                    && area.width > 0
                    && area.height > 0
                    && mouse.column >= area.x
                    && mouse.column < area.x + area.width
                    && mouse.row >= area.y
                    && mouse.row < area.y + area.height
                {
                    // Clicked on border of tab content — just focus tabs
                    self.focus_on_manager = false;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                } else {
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging_divider && self.host_panel_visible {
                    // Resize host panel by dragging the divider
                    let new_width = mouse.column.saturating_sub(self.host_panel_area.x);
                    self.host_panel_width = new_width.clamp(15, 80);
                } else if self.is_pty_mouse_mode_active() {
                    // Forward drag to PTY for TUI app (button 32 = motion with left button)
                    // PTY mouse mode takes priority over selection
                    let mode = self.pty_mouse_mode();
                    if mode == vt100::MouseProtocolMode::AnyMotion || mode == vt100::MouseProtocolMode::ButtonMotion {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(32, col, row, false)?;
                        }
                    }
                } else if self.is_selecting && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    // Cossh text selection drag (only when no TUI app has mouse mode)
                    self.selection_dragged = true;
                    let area = self.tab_content_area;
                    let clamped_col = mouse.column.max(area.x + 1).min(area.x + area.width.saturating_sub(2));
                    let clamped_row = mouse.row.max(area.y + 1).min(area.y + area.height.saturating_sub(2));
                    let vt_row = clamped_row.saturating_sub(area.y + 1);
                    let vt_col = clamped_col.saturating_sub(area.x + 1);
                    let scroll_offset = self.tabs[self.selected_tab].scroll_offset;
                    let abs_row = vt_row as i64 - scroll_offset as i64;
                    self.selection_end = Some((abs_row, vt_col));
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // Forward right-click to PTY if mouse mode is active
                if self.is_pty_mouse_mode_active() {
                    if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                        self.send_mouse_to_pty(2, col, row, false)?;
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging_divider {
                    self.is_dragging_divider = false;
                } else if self.is_selecting {
                    // Cossh text selection release
                    self.is_selecting = false;
                    if self.selection_dragged {
                        // Mouse was dragged — copy selection (even single char)
                        self.copy_selection_to_clipboard();
                    } else {
                        // Just a click, no drag — clear selection
                        self.selection_start = None;
                        self.selection_end = None;
                    }
                } else if self.is_pty_mouse_mode_active() {
                    // Forward release to PTY for TUI app
                    let mode = self.pty_mouse_mode();
                    if mode != vt100::MouseProtocolMode::Press {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(0, col, row, true)?;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                // Check if scrolling over the tab bar
                let tab_area = self.tab_bar_area;
                let host_area = self.host_list_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    // Scroll up on tab bar = previous tab
                    if self.selected_tab > 0 {
                        self.selected_tab -= 1;
                        self.focus_on_manager = false;
                        self.selection_start = None;
                        self.selection_end = None;
                        self.ensure_tab_visible();
                    }
                } else if host_area.width > 0
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row >= host_area.y
                    && mouse.row < host_area.y + host_area.height
                {
                    // Scroll host list up
                    if !self.filtered_hosts.is_empty() && self.selected_host > 0 {
                        self.selected_host = self.selected_host.saturating_sub(3);
                        self.host_list_state.select(Some(self.selected_host));
                    }
                } else if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    // If PTY wants mouse events, forward scroll to PTY
                    if self.is_pty_mouse_mode_active() {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(64, col, row, false)?;
                        }
                    } else {
                        // Scroll wheel up: scroll back through PTY history
                        let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_add(3).min(max_scrollback);
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                // Check if scrolling over the tab bar
                let tab_area = self.tab_bar_area;
                let host_area = self.host_list_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    // Scroll down on tab bar = next tab
                    if self.selected_tab < self.tabs.len() - 1 {
                        self.selected_tab += 1;
                        self.focus_on_manager = false;
                        self.selection_start = None;
                        self.selection_end = None;
                        self.ensure_tab_visible();
                    }
                } else if host_area.width > 0
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row >= host_area.y
                    && mouse.row < host_area.y + host_area.height
                {
                    // Scroll host list down
                    if !self.filtered_hosts.is_empty() && self.selected_host < self.filtered_hosts.len().saturating_sub(1) {
                        self.selected_host = (self.selected_host + 3).min(self.filtered_hosts.len().saturating_sub(1));
                        self.host_list_state.select(Some(self.selected_host));
                    }
                } else if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    // If PTY wants mouse events, forward scroll to PTY
                    if self.is_pty_mouse_mode_active() {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(65, col, row, false)?;
                        }
                    } else {
                        // Scroll wheel down: scroll towards live PTY view
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_sub(3);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                // Forward middle button to PTY if mouse mode is active
                if self.is_pty_mouse_mode_active() {
                    if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                        self.send_mouse_to_pty(1, col, row, false)?;
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Middle) => {
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode != vt100::MouseProtocolMode::Press {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(1, col, row, true)?;
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Right) => {
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode != vt100::MouseProtocolMode::Press {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(2, col, row, true)?;
                        }
                    }
                }
            }
            MouseEventKind::Moved => {
                // Forward motion to PTY if AnyMotion mode is active
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode == vt100::MouseProtocolMode::AnyMotion {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(35, col, row, false)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Get the maximum scrollback offset available for a given tab
    fn max_scrollback_for_tab(&self, tab_idx: usize) -> usize {
        if tab_idx >= self.tabs.len() {
            return 0;
        }
        if let Some(session) = &self.tabs[tab_idx].session {
            if let Ok(mut parser) = session.parser.lock() {
                parser.set_scrollback(usize::MAX);
                let max = parser.screen().scrollback();
                parser.set_scrollback(0);
                max
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Check if the current tab's PTY session has mouse mode enabled
    fn is_pty_mouse_mode_active(&self) -> bool {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return false;
        }
        if let Some(session) = &self.tabs[self.selected_tab].session {
            if let Ok(parser) = session.parser.lock() {
                return parser.screen().mouse_protocol_mode()
                    != vt100::MouseProtocolMode::None;
            }
        }
        false
    }

    /// Get the mouse protocol encoding for the current tab's PTY
    fn pty_mouse_encoding(&self) -> vt100::MouseProtocolEncoding {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return vt100::MouseProtocolEncoding::Default;
        }
        if let Some(session) = &self.tabs[self.selected_tab].session {
            if let Ok(parser) = session.parser.lock() {
                return parser.screen().mouse_protocol_encoding();
            }
        }
        vt100::MouseProtocolEncoding::Default
    }

    /// Get the mouse protocol mode for the current tab's PTY
    fn pty_mouse_mode(&self) -> vt100::MouseProtocolMode {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return vt100::MouseProtocolMode::None;
        }
        if let Some(session) = &self.tabs[self.selected_tab].session {
            if let Ok(parser) = session.parser.lock() {
                return parser.screen().mouse_protocol_mode();
            }
        }
        vt100::MouseProtocolMode::None
    }

    /// Check if a mouse event is inside the tab content area and return VT100 coords (1-based)
    fn mouse_to_vt_coords(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let area = self.tab_content_area;
        if area.width > 2
            && area.height > 2
            && column > area.x
            && column < area.x + area.width - 1
            && row > area.y
            && row < area.y + area.height - 1
        {
            // VT100 mouse coords are 1-based
            let vt_col = column - area.x; // 1-based (border is at area.x)
            let vt_row = row - area.y; // 1-based (border is at area.y)
            Some((vt_col, vt_row))
        } else {
            None
        }
    }

    /// Forward a mouse event to the PTY using the appropriate encoding
    fn send_mouse_to_pty(&mut self, button: u8, col: u16, row: u16, is_release: bool) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }
        let encoding = self.pty_mouse_encoding();
        let bytes = match encoding {
            vt100::MouseProtocolEncoding::Sgr => {
                // SGR encoding: \x1b[<button;col;row;M (press) or m (release)
                let suffix = if is_release { 'm' } else { 'M' };
                format!("\x1b[<{};{};{}{}", button, col, row, suffix).into_bytes()
            }
            _ => {
                // Default/UTF8 encoding: \x1b[M + (button+32) + (col+32) + (row+32)
                if is_release {
                    // Release in default encoding: button 3
                    let cb = (3u8 + 32) as char;
                    let cx = ((col as u8).saturating_add(32)) as char;
                    let cy = ((row as u8).saturating_add(32)) as char;
                    format!("\x1b[M{}{}{}", cb, cx, cy).into_bytes()
                } else {
                    let cb = (button.saturating_add(32)) as char;
                    let cx = ((col as u8).saturating_add(32)) as char;
                    let cy = ((row as u8).saturating_add(32)) as char;
                    format!("\x1b[M{}{}{}", cb, cx, cy).into_bytes()
                }
            }
        };

        let tab = &mut self.tabs[self.selected_tab];
        if let Some(session) = &mut tab.session {
            if let Ok(mut writer) = session.writer.lock() {
                let _ = writer.write_all(&bytes);
            }
        }
        Ok(())
    }

    /// Send keyboard input to the active PTY
    pub(super) fn send_key_to_pty(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }

        let tab = &mut self.tabs[self.selected_tab];
        if let Some(session) = &mut tab.session {
            let bytes = match key.code {
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        // Send control character
                        if c.is_ascii_alphabetic() {
                            vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]
                        } else {
                            vec![c as u8]
                        }
                    } else {
                        c.to_string().into_bytes()
                    }
                }
                KeyCode::Enter => vec![b'\r'],
                KeyCode::Backspace => vec![127],
                KeyCode::Tab => vec![b'\t'],
                KeyCode::Esc => vec![27],
                KeyCode::Up => b"\x1b[A".to_vec(),
                KeyCode::Down => b"\x1b[B".to_vec(),
                KeyCode::Right => b"\x1b[C".to_vec(),
                KeyCode::Left => b"\x1b[D".to_vec(),
                KeyCode::Home => b"\x1b[H".to_vec(),
                KeyCode::End => b"\x1b[F".to_vec(),
                KeyCode::PageUp => b"\x1b[5~".to_vec(),
                KeyCode::PageDown => b"\x1b[6~".to_vec(),
                KeyCode::Delete => b"\x1b[3~".to_vec(),
                KeyCode::Insert => b"\x1b[2~".to_vec(),
                _ => return Ok(()),
            };

            // Write to PTY using stored writer
            if let Ok(mut writer) = session.writer.lock() {
                let _ = writer.write_all(&bytes);
            }
        }

        Ok(())
    }

    /// Calculate the width of a tab title in characters
    fn tab_display_width(&self, idx: usize) -> usize {
        if idx >= self.tabs.len() {
            return 0;
        }
        let is_selected = idx == self.selected_tab && !self.focus_on_manager;
        // Selected: " [title ×] " = title.len() + 6
        // Unselected: " title × " = title.len() + 4
        if is_selected { self.tabs[idx].title.len() + 6 } else { self.tabs[idx].title.len() + 4 }
    }

    /// Ensure the selected tab is visible within the tab bar by adjusting tab_scroll_offset
    pub(super) fn ensure_tab_visible(&mut self) {
        if self.tabs.is_empty() {
            self.tab_scroll_offset = 0;
            return;
        }

        let tab_bar_width = self.tab_bar_area.width.saturating_sub(2) as usize; // subtract borders
        if tab_bar_width == 0 {
            return;
        }

        // Calculate the pixel position range of the selected tab
        let mut start_pos: usize = 0;
        for i in 0..self.selected_tab {
            start_pos += self.tab_display_width(i);
            if i < self.tabs.len() - 1 {
                start_pos += 1; // separator "│"
            }
        }
        let end_pos = start_pos + self.tab_display_width(self.selected_tab);

        // Adjust scroll to ensure selected tab is visible
        if start_pos < self.tab_scroll_offset {
            self.tab_scroll_offset = start_pos;
        } else if end_pos > self.tab_scroll_offset + tab_bar_width {
            self.tab_scroll_offset = end_pos.saturating_sub(tab_bar_width);
        }
    }

    /// Update terminal search matches based on current query
    fn update_terminal_search(&mut self) {
        self.terminal_search_matches.clear();
        self.terminal_search_current = 0;

        if self.terminal_search_query.is_empty() || self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab = &self.tabs[self.selected_tab];
        if let Some(session) = &tab.session {
            if let Ok(mut parser) = session.parser.lock() {
                // Get max scrollback to know how much history exists
                parser.set_scrollback(usize::MAX);
                let max_scrollback = parser.screen().scrollback();
                
                let query_lower = self.terminal_search_query.to_lowercase();
                
                // Search through entire history
                // Strategy: iterate through each scrollback position from max to 0
                // At scrollback > 0, search only row 0 (the top line that changes with each scroll)
                // At scrollback = 0, search all rows (the current live screen)
                for scrollback_pos in (0..=max_scrollback).rev() {
                    parser.set_scrollback(scrollback_pos);
                    let screen = parser.screen();
                    let (rows, cols) = screen.size();
                    
                    // Determine which rows to search at this scrollback position
                    let search_rows: Vec<u16> = if scrollback_pos == 0 {
                        // At live view, search all rows to get the remaining lines
                        (0..rows).collect()
                    } else {
                        // At scrollback > 0, search only row 0 to avoid duplicates
                        vec![0]
                    };
                    
                    for &row in &search_rows {
                        // Extract text from this row, tracking column positions
                        let mut row_text = String::new();
                        let mut col_to_pos = Vec::new(); // Maps column to string position
                        
                        for col in 0..cols {
                            col_to_pos.push(row_text.len());
                            if let Some(cell) = screen.cell(row, col) {
                                if cell.has_contents() {
                                    row_text.push_str(&cell.contents());
                                } else {
                                    row_text.push(' ');
                                }
                            } else {
                                row_text.push(' ');
                            }
                        }

                        // Search for query in row text (case-insensitive)
                        let row_text_lower = row_text.to_lowercase();
                        let mut search_start = 0;
                        while let Some(pos) = row_text_lower[search_start..].find(&query_lower) {
                            let match_pos = search_start + pos;
                            
                            // Find which column this match starts at
                            let mut match_col = 0;
                            for (col_idx, &string_pos) in col_to_pos.iter().enumerate() {
                                if string_pos == match_pos {
                                    match_col = col_idx;
                                    break;
                                } else if string_pos > match_pos {
                                    break;
                                } else {
                                    match_col = col_idx;
                                }
                            }
                            
                            // Convert to absolute row
                            // At scrollback=S, row R has absolute position: R - S
                            let abs_row = row as i64 - scrollback_pos as i64;
                            self.terminal_search_matches.push((abs_row, match_col as u16, query_lower.chars().count()));
                            
                            search_start = match_pos + 1; // Allow overlapping matches
                        }
                    }
                }

                // Reset scrollback to current view
                parser.set_scrollback(tab.scroll_offset);
            }
        }
        
        // Matches are already in order from oldest to newest
        // If we have matches, scroll to the first one
        if !self.terminal_search_matches.is_empty() {
            self.scroll_to_search_match();
        }
    }

    /// Scroll to the current search match
    fn scroll_to_search_match(&mut self) {
        if self.terminal_search_matches.is_empty() || self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let (abs_row, _, _) = self.terminal_search_matches[self.terminal_search_current];
        let tab = &mut self.tabs[self.selected_tab];

        // Calculate the screen row from absolute row
        // Relationship: abs_row = screen_row - scroll_offset
        // Therefore: scroll_offset = screen_row - abs_row
        
        // We want to position the match at target_screen_row
        let tab_height = self.tab_content_area.height.saturating_sub(2) as i64;
        
        if let Some(session) = &tab.session {
            if let Ok(mut parser) = session.parser.lock() {
                let max_scrollback = {
                    parser.set_scrollback(usize::MAX);
                    let max = parser.screen().scrollback();
                    parser.set_scrollback(0);
                    max
                };

                // Target: put match at 1/3 from top of screen
                let target_screen_row = tab_height / 3;
                
                // Calculate needed scroll: scroll_offset = target_screen_row - abs_row
                let needed_scroll = target_screen_row - abs_row;

                if needed_scroll < 0 {
                    // Match is in the "future" (beyond current live view), stay at live view
                    tab.scroll_offset = 0;
                } else {
                    tab.scroll_offset = (needed_scroll as usize).min(max_scrollback);
                }
            }
        }
    }
}
