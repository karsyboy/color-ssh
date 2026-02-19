//! Mouse input handling and PTY mouse forwarding.

use crate::tui::state::{HOST_PANEL_MAX_WIDTH, HOST_PANEL_MIN_WIDTH};
use crate::tui::terminal_emulator;
use crate::tui::{HostTreeRowKind, SessionManager};
use crossterm::event::{self, KeyModifiers, MouseButton, MouseEventKind};
use std::io;
use std::time::Instant;

impl SessionManager {
    // Top-level mouse routing for host panel, tab bar, and terminal area.
    /// Handle mouse events.
    pub(crate) fn handle_mouse(&mut self, mouse: event::MouseEvent) -> io::Result<()> {
        if self.quick_connect.is_some() {
            self.handle_quick_connect_mouse(mouse);
            return Ok(());
        }

        if self.current_tab_search().map(|search_state| search_state.active).unwrap_or(false)
            && self.is_pty_mouse_mode_active()
            && let Some(search) = self.current_tab_search_mut()
        {
            search.active = false;
            search.query.clear();
            search.matches.clear();
            search.current = 0;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.is_dragging_host_scrollbar = false;
                self.is_dragging_host_info_divider = false;

                let divider_col = self.host_panel_area.x + self.host_panel_area.width.saturating_sub(1);
                if self.host_panel_visible && self.host_panel_area.width > 0 && mouse.column == divider_col {
                    self.is_dragging_divider = true;
                    self.clear_selection_state();
                    return Ok(());
                }

                if self.host_panel_visible && self.host_info_visible && self.host_info_area.height > 0 {
                    let divider_row = self.host_info_area.y;
                    let host_content_right = self.host_panel_area.x + self.host_panel_area.width.saturating_sub(1);
                    if mouse.row == divider_row && mouse.column >= self.host_panel_area.x && mouse.column < host_content_right {
                        self.focus_manager_panel();
                        self.is_dragging_host_info_divider = true;
                        self.clear_selection_state();
                        return Ok(());
                    }
                }

                let host_area = self.host_list_area;
                if self.host_panel_visible
                    && host_area.width > 0
                    && host_area.height > 0
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row >= host_area.y
                    && mouse.row < host_area.y + host_area.height
                {
                    if let Some(scrollbar_x) = self.host_scrollbar_x()
                        && mouse.column == scrollbar_x
                    {
                        self.focus_manager_panel();
                        self.is_dragging_host_scrollbar = true;
                        self.set_host_scroll_from_scrollbar_row(mouse.row);
                        self.clear_selection_state();
                        return Ok(());
                    }

                    let clicked_row = (mouse.row - host_area.y) as usize;
                    let clicked_index = self.host_scroll_offset + clicked_row;
                    if clicked_index < self.visible_host_rows.len() {
                        self.set_selected_row(clicked_index);
                        self.focus_manager_panel();

                        let row_kind = self.visible_host_rows[clicked_index].kind;
                        if let HostTreeRowKind::Folder(folder_id) = row_kind {
                            self.toggle_folder(folder_id);
                            self.last_click = None;
                        } else {
                            let now = Instant::now();
                            let is_double_click = if let Some((last_time, last_col, last_row)) = self.last_click {
                                now.duration_since(last_time).as_millis() < 400 && last_col == mouse.column && last_row == mouse.row
                            } else {
                                false
                            };

                            if is_double_click {
                                self.last_click = None;
                                self.select_host_to_connect();
                            } else {
                                self.last_click = Some((now, mouse.column, mouse.row));
                            }
                        }

                        self.clear_selection_state();
                        return Ok(());
                    }

                    self.focus_manager_panel();
                    self.clear_selection_state();
                    return Ok(());
                }

                let panel_area = self.host_panel_area;
                if self.host_panel_visible
                    && panel_area.width > 0
                    && panel_area.height > 0
                    && mouse.column >= panel_area.x
                    && mouse.column < panel_area.x + panel_area.width
                    && mouse.row >= panel_area.y
                    && mouse.row < panel_area.y + panel_area.height
                {
                    self.focus_manager_panel();
                    self.clear_selection_state();
                    return Ok(());
                }

                let tab_area = self.tab_bar_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && tab_area.height > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    let visual_col = (mouse.column - tab_area.x) as usize;
                    let tab_widths: Vec<usize> = self.tabs.iter().enumerate().map(|(idx, _)| self.tab_display_width(idx)).collect();
                    let available_width = tab_area.width as usize;
                    self.tab_scroll_offset = self.normalize_tab_scroll_offset(self.tab_scroll_offset, available_width);
                    let has_left_overflow = self.prev_tab_scroll_offset(self.tab_scroll_offset, available_width).is_some();
                    let left_slot = if has_left_overflow { 1 } else { 0 };
                    let has_right_overflow = self.next_tab_scroll_offset(self.tab_scroll_offset, available_width).is_some();
                    let right_slot = if has_right_overflow { 1 } else { 0 };
                    let visible_tab_width = available_width.saturating_sub(left_slot + right_slot);

                    if has_left_overflow && visual_col == 0 {
                        if let Some(prev_offset) = self.prev_tab_scroll_offset(self.tab_scroll_offset, available_width) {
                            self.tab_scroll_offset = prev_offset;
                        }
                        return Ok(());
                    }
                    if has_right_overflow && visual_col == available_width.saturating_sub(1) {
                        if let Some(next_offset) = self.next_tab_scroll_offset(self.tab_scroll_offset, available_width) {
                            self.tab_scroll_offset = next_offset;
                        }
                        return Ok(());
                    }

                    if visual_col < left_slot || visual_col >= left_slot + visible_tab_width {
                        self.focus_on_manager = false;
                        self.clear_selection_state();
                        return Ok(());
                    }
                    let local_col = visual_col - left_slot;

                    let mut running_start = 0usize;
                    let mut first_visible_idx = 0usize;
                    while first_visible_idx < self.tabs.len() && running_start + tab_widths[first_visible_idx] <= self.tab_scroll_offset {
                        running_start += tab_widths[first_visible_idx];
                        first_visible_idx += 1;
                    }

                    let mut used = 0usize;
                    let mut idx = first_visible_idx;
                    while idx < self.tabs.len() && used < visible_tab_width {
                        let tab_width = tab_widths[idx];
                        let visible_end = (used + tab_width).min(visible_tab_width);
                        if local_col < visible_end {
                            let close_pos = used + self.tab_title_display_width(idx) + 1;
                            if close_pos < visible_end && local_col == close_pos {
                                self.selected_tab = idx;
                                self.close_current_tab();
                                if self.tabs.is_empty() {
                                    self.selected_tab = 0;
                                }
                            } else {
                                self.selected_tab = idx;
                                self.focus_on_manager = false;
                            }
                            self.ensure_tab_visible();
                            self.clear_selection_state();
                            return Ok(());
                        }
                        used += tab_width;
                        idx += 1;
                    }

                    self.focus_on_manager = false;
                    self.clear_selection_state();
                    return Ok(());
                }

                let area = self.tab_content_area;
                if !self.tabs.is_empty()
                    && self.selected_tab < self.tabs.len()
                    && area.width > 0
                    && area.height > 0
                    && mouse.column >= area.x
                    && mouse.column < area.x + area.width
                    && mouse.row >= area.y
                    && mouse.row < area.y + area.height
                {
                    self.focus_on_manager = false;
                    self.search_mode = false;
                    let alt_held = mouse.modifiers.contains(KeyModifiers::ALT);

                    if self.is_pty_mouse_mode_active() && !alt_held {
                        if self.tabs[self.selected_tab].scroll_offset > 0 {
                            self.tabs[self.selected_tab].scroll_offset = 0;
                        }
                        self.clear_selection_state();
                        self.selection_dragged = false;
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(0, col, row, false)?;
                        }
                    } else if !self.is_pty_mouse_mode_active() || alt_held {
                        let vt_row = mouse.row.saturating_sub(area.y);
                        let vt_col = mouse.column.saturating_sub(area.x);
                        let scroll_offset = self.tabs[self.selected_tab].scroll_offset;
                        let abs_row = vt_row as i64 - scroll_offset as i64;
                        self.selection_start = Some((abs_row, vt_col));
                        self.selection_end = Some((abs_row, vt_col));
                        self.is_selecting = true;
                        self.selection_dragged = false;
                    }
                } else {
                    self.clear_selection_state();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging_divider && self.host_panel_visible {
                    let new_width = mouse.column.saturating_sub(self.host_panel_area.x).saturating_add(1);
                    self.host_panel_width = new_width.clamp(HOST_PANEL_MIN_WIDTH, HOST_PANEL_MAX_WIDTH);
                } else if self.is_dragging_host_info_divider && self.host_panel_visible && self.host_info_visible {
                    const MIN_HOST_LIST_HEIGHT: u16 = 4;
                    const MIN_HOST_INFO_HEIGHT: u16 = 3;

                    let content_top = self.host_panel_area.y;
                    let content_height = self.host_panel_area.height;
                    if content_height > MIN_HOST_LIST_HEIGHT {
                        let min_split = content_top.saturating_add(MIN_HOST_LIST_HEIGHT);
                        let max_split = content_top + content_height.saturating_sub(MIN_HOST_INFO_HEIGHT);
                        let split_row = mouse.row.clamp(min_split, max_split);
                        let list_height = split_row.saturating_sub(content_top);
                        self.host_info_height = content_height.saturating_sub(list_height);
                    }
                } else if self.is_dragging_host_scrollbar {
                    self.set_host_scroll_from_scrollbar_row(mouse.row);
                } else if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if (mode == terminal_emulator::MouseProtocolMode::AnyMotion || mode == terminal_emulator::MouseProtocolMode::ButtonMotion)
                        && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                    {
                        self.send_mouse_to_pty(32, col, row, false)?;
                    }
                } else if self.is_selecting && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    self.selection_dragged = true;
                    let area = self.tab_content_area;
                    if area.width == 0 || area.height == 0 {
                        return Ok(());
                    }

                    let top_row = area.y;
                    let bottom_row = area.y + area.height.saturating_sub(1);
                    if mouse.row <= top_row {
                        let edge_distance = top_row.saturating_sub(mouse.row).saturating_add(1) as usize;
                        let step = edge_distance.min(10);
                        let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_add(step).min(max_scrollback);
                    } else if mouse.row >= bottom_row {
                        let edge_distance = mouse.row.saturating_sub(bottom_row).saturating_add(1) as usize;
                        let step = edge_distance.min(10);
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_sub(step);
                    }

                    let clamped_col = mouse.column.max(area.x).min(area.x + area.width.saturating_sub(1));
                    let clamped_row = mouse.row.max(top_row).min(bottom_row);
                    let vt_row = clamped_row.saturating_sub(area.y);
                    let vt_col = clamped_col.saturating_sub(area.x);
                    let scroll_offset = self.tabs[self.selected_tab].scroll_offset;
                    let abs_row = vt_row as i64 - scroll_offset as i64;
                    self.selection_end = Some((abs_row, vt_col));
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if self.is_pty_mouse_mode_active()
                    && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                {
                    self.send_mouse_to_pty(2, col, row, false)?;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging_divider {
                    self.is_dragging_divider = false;
                } else if self.is_dragging_host_info_divider {
                    self.is_dragging_host_info_divider = false;
                } else if self.is_dragging_host_scrollbar {
                    self.is_dragging_host_scrollbar = false;
                } else if self.is_selecting {
                    self.is_selecting = false;
                    if self.selection_dragged {
                        self.copy_selection_to_clipboard();
                    } else {
                        self.clear_selection_state();
                    }
                } else if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode != terminal_emulator::MouseProtocolMode::Press
                        && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                    {
                        self.send_mouse_to_pty(0, col, row, true)?;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                let tab_area = self.tab_bar_area;
                let host_area = self.host_list_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    if self.selected_tab > 0 {
                        self.selected_tab -= 1;
                        self.focus_on_manager = false;
                        self.clear_selection_state();
                        self.ensure_tab_visible();
                    }
                } else if host_area.width > 0
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row >= host_area.y
                    && mouse.row < host_area.y + host_area.height
                {
                    if self.visible_host_row_count() > 0 && self.selected_host_row > 0 {
                        self.set_selected_row(self.selected_host_row.saturating_sub(1));
                    }
                } else if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    if self.is_pty_mouse_mode_active() {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(64, col, row, false)?;
                        }
                    } else {
                        let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_add(3).min(max_scrollback);
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                let tab_area = self.tab_bar_area;
                let host_area = self.host_list_area;
                if !self.tabs.is_empty()
                    && tab_area.width > 0
                    && mouse.column >= tab_area.x
                    && mouse.column < tab_area.x + tab_area.width
                    && mouse.row >= tab_area.y
                    && mouse.row < tab_area.y + tab_area.height
                {
                    if self.selected_tab < self.tabs.len() - 1 {
                        self.selected_tab += 1;
                        self.focus_on_manager = false;
                        self.clear_selection_state();
                        self.ensure_tab_visible();
                    }
                } else if host_area.width > 0
                    && mouse.column >= host_area.x
                    && mouse.column < host_area.x + host_area.width
                    && mouse.row >= host_area.y
                    && mouse.row < host_area.y + host_area.height
                {
                    let row_count = self.visible_host_row_count();
                    if row_count > 0 && self.selected_host_row < row_count.saturating_sub(1) {
                        self.set_selected_row((self.selected_host_row + 1).min(row_count.saturating_sub(1)));
                    }
                } else if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    if self.is_pty_mouse_mode_active() {
                        if let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row) {
                            self.send_mouse_to_pty(65, col, row, false)?;
                        }
                    } else {
                        let tab = &mut self.tabs[self.selected_tab];
                        tab.scroll_offset = tab.scroll_offset.saturating_sub(3);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                if self.is_pty_mouse_mode_active()
                    && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                {
                    self.send_mouse_to_pty(1, col, row, false)?;
                }
            }
            MouseEventKind::Up(MouseButton::Middle) => {
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode != terminal_emulator::MouseProtocolMode::Press
                        && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                    {
                        self.send_mouse_to_pty(1, col, row, true)?;
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Right) => {
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode != terminal_emulator::MouseProtocolMode::Press
                        && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                    {
                        self.send_mouse_to_pty(2, col, row, true)?;
                    }
                }
            }
            MouseEventKind::Moved => {
                if self.is_pty_mouse_mode_active() {
                    let mode = self.pty_mouse_mode();
                    if mode == terminal_emulator::MouseProtocolMode::AnyMotion
                        && let Some((col, row)) = self.mouse_to_vt_coords(mouse.column, mouse.row)
                    {
                        self.send_mouse_to_pty(35, col, row, false)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // Scrollback helpers.
    pub(crate) fn max_scrollback_for_tab(&self, tab_idx: usize) -> usize {
        if tab_idx >= self.tabs.len() {
            return 0;
        }
        if let Some(session) = &self.tabs[tab_idx].session {
            if let Ok(parser) = session.parser.lock() {
                parser.screen().scrollback()
            } else {
                0
            }
        } else {
            0
        }
    }

    // PTY mouse mode helpers.
    pub(crate) fn is_pty_mouse_mode_active(&self) -> bool {
        self.pty_mouse_mode() != terminal_emulator::MouseProtocolMode::None
    }

    fn pty_mouse_protocol(&self) -> (terminal_emulator::MouseProtocolMode, terminal_emulator::MouseProtocolEncoding) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return (terminal_emulator::MouseProtocolMode::None, terminal_emulator::MouseProtocolEncoding::Default);
        }
        if let Some(session) = &self.tabs[self.selected_tab].session
            && let Ok(parser) = session.parser.lock()
        {
            return parser.screen().mouse_protocol();
        }
        (terminal_emulator::MouseProtocolMode::None, terminal_emulator::MouseProtocolEncoding::Default)
    }

    fn pty_mouse_mode(&self) -> terminal_emulator::MouseProtocolMode {
        self.pty_mouse_protocol().0
    }

    // Convert screen coords to VT (1-based) coordinates.
    fn mouse_to_vt_coords(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let area = self.tab_content_area;
        if area.width > 0 && area.height > 0 && column >= area.x && column < area.x + area.width && row >= area.y && row < area.y + area.height {
            let vt_col = (column - area.x) + 1;
            let vt_row = (row - area.y) + 1;
            Some((vt_col, vt_row))
        } else {
            None
        }
    }

    // Encode mouse reporting bytes for default and SGR modes.
    fn encode_mouse_event_bytes(encoding: terminal_emulator::MouseProtocolEncoding, button: u8, col: u16, row: u16, is_release: bool) -> Vec<u8> {
        match encoding {
            terminal_emulator::MouseProtocolEncoding::Sgr => {
                let suffix = if is_release { 'm' } else { 'M' };
                format!("\x1b[<{};{};{}{}", button, col, row, suffix).into_bytes()
            }
            _ => {
                // Legacy X10 encoding only supports 8-bit coordinates. Clamp to avoid wraparound.
                let clamped_col = col.clamp(1, 223) as u8;
                let clamped_row = row.clamp(1, 223) as u8;
                let cb = if is_release { 3u8 + 32 } else { button.saturating_add(32) };
                let cx = clamped_col.saturating_add(32);
                let cy = clamped_row.saturating_add(32);
                vec![0x1b, b'[', b'M', cb, cx, cy]
            }
        }
    }

    // Send encoded mouse bytes to active PTY.
    fn send_mouse_to_pty(&mut self, button: u8, col: u16, row: u16, is_release: bool) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }
        let encoding = self.pty_mouse_protocol().1;
        let bytes = Self::encode_mouse_event_bytes(encoding, button, col, row, is_release);
        self.write_bytes_to_active_pty(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionManager;
    use crate::ssh_config::SshHost;
    use crate::tui::terminal_emulator::MouseProtocolEncoding;
    use crate::tui::{HostTab, TerminalSearchState};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    fn app_with_tabs(titles: &[&str]) -> SessionManager {
        let mut app = SessionManager::new_for_tests();
        app.host_panel_visible = false;
        app.host_panel_area = Rect::default();
        app.host_list_area = Rect::default();
        app.tab_bar_area = Rect::new(0, 0, 40, 1);
        for title in titles {
            app.tabs.push(HostTab {
                host: SshHost::new((*title).to_string()),
                title: (*title).to_string(),
                session: None,
                scroll_offset: 0,
                terminal_search: TerminalSearchState::default(),
                force_ssh_logging: false,
                last_pty_size: None,
            });
        }
        app
    }

    #[test]
    fn encode_mouse_event_bytes_sgr_press_and_release() {
        let press = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Sgr, 0, 10, 5, false);
        let release = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Sgr, 0, 10, 5, true);

        assert_eq!(press, b"\x1b[<0;10;5M".to_vec());
        assert_eq!(release, b"\x1b[<0;10;5m".to_vec());
    }

    #[test]
    fn encode_mouse_event_bytes_default_clamps_large_coords() {
        let bytes = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Default, 0, 500, 900, false);
        assert_eq!(bytes, vec![0x1b, b'[', b'M', 32, 255, 255]);
    }

    #[test]
    fn closes_wide_title_tab_when_clicking_close_glyph() {
        let mut app = app_with_tabs(&["a界", "next"]);
        let close_click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(close_click).expect("mouse handling");
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].title, "next");
    }

    #[test]
    fn selects_tab_when_clicking_title_region_not_close_glyph() {
        let mut app = app_with_tabs(&["one", "a界"]);
        let select_second_tab = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(select_second_tab).expect("mouse handling");
        assert_eq!(app.selected_tab, 1);
        assert_eq!(app.tabs.len(), 2);
    }

    #[test]
    fn scroll_markers_move_tab_strip_left_and_right() {
        let mut app = app_with_tabs(&["one", "two", "three", "four"]);
        app.tab_bar_area = Rect::new(0, 0, 10, 1);
        app.tab_scroll_offset = 0;

        let right_marker_click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 9,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(right_marker_click).expect("right marker click");
        assert!(app.tab_scroll_offset > 0);

        let left_marker_click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(left_marker_click).expect("left marker click");
        assert_eq!(app.tab_scroll_offset, 0);
    }
}
