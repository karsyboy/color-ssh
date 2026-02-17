//! Terminal-tabs and root layout rendering.

use crate::tui::SessionManager;
use crate::tui::features::selection::extract::is_cell_in_selection;
use crate::tui::features::terminal_search::render_highlight::build_search_row_ranges;
use crate::tui::ui::theme::{display_width, truncate_to_display_width};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Convert VT100 color to Ratatui color.
fn vt100_to_ratatui_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => match idx {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::White,
            _ => Color::Indexed(idx),
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn draw_vertical_rule(frame: &mut Frame, x: u16, y: u16, height: u16, style: Style) {
    if height == 0 {
        return;
    }
    let frame_area = frame.area();
    if x < frame_area.x || x >= frame_area.x + frame_area.width {
        return;
    }
    let buf = frame.buffer_mut();
    let end_y = y.saturating_add(height).min(frame_area.y + frame_area.height);
    for row in y..end_y {
        let cell = &mut buf[(x, row)];
        cell.set_symbol("│");
        cell.set_style(style);
    }
}

fn draw_horizontal_rule(frame: &mut Frame, y: u16, x: u16, width: u16, style: Style) {
    if width == 0 {
        return;
    }
    let frame_area = frame.area();
    if y < frame_area.y || y >= frame_area.y + frame_area.height {
        return;
    }
    let buf = frame.buffer_mut();
    let end_x = x.saturating_add(width).min(frame_area.x + frame_area.width);
    for col in x..end_x {
        let cell = &mut buf[(col, y)];
        cell.set_symbol("─");
        cell.set_style(style);
    }
}

impl SessionManager {
    /// Render the full UI.
    pub(crate) fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();
        self.handle_terminal_resize(size.width, size.height);
        let root_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)])
            .split(size);
        let content_area = root_chunks[0];
        let separator_area = root_chunks[1];
        let status_area = root_chunks[2];

        let (main_chunks, show_host_panel) = if self.host_panel_visible {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(self.host_panel_width), Constraint::Min(0)])
                .split(content_area);
            (chunks, true)
        } else {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(0), Constraint::Min(0)])
                .split(content_area);
            (chunks, false)
        };

        if show_host_panel {
            let host_panel_area = main_chunks[0];
            let host_content_area = Rect::new(
                host_panel_area.x,
                host_panel_area.y,
                host_panel_area.width.saturating_sub(1),
                host_panel_area.height,
            );

            self.host_panel_area = host_panel_area;
            self.host_info_area = Rect::default();

            if host_content_area.width > 0 {
                const MIN_HOST_LIST_HEIGHT: u16 = 4;
                const MIN_HOST_INFO_HEIGHT: u16 = 3;

                let mut host_list_area = host_content_area;

                if self.host_info_visible && host_content_area.height > MIN_HOST_LIST_HEIGHT {
                    let max_info_height = host_content_area.height.saturating_sub(MIN_HOST_LIST_HEIGHT);
                    let min_info_height = MIN_HOST_INFO_HEIGHT.min(max_info_height);

                    if self.host_info_height < min_info_height {
                        self.host_info_height = min_info_height;
                    } else if self.host_info_height > max_info_height {
                        self.host_info_height = max_info_height;
                    }

                    let info_height = self.host_info_height.clamp(min_info_height, max_info_height);
                    let list_height = host_content_area.height.saturating_sub(info_height);
                    host_list_area = Rect::new(host_content_area.x, host_content_area.y, host_content_area.width, list_height);
                    self.host_info_area = Rect::new(
                        host_content_area.x,
                        host_content_area.y.saturating_add(list_height),
                        host_content_area.width,
                        info_height,
                    );
                }

                self.render_host_list(frame, host_list_area);

                if self.host_info_visible && self.host_info_area.height > 0 {
                    draw_horizontal_rule(
                        frame,
                        self.host_info_area.y,
                        self.host_info_area.x,
                        self.host_info_area.width,
                        if self.is_dragging_host_info_divider {
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    );
                    self.render_host_info(frame, self.host_info_area);
                } else {
                    self.host_info_area = Rect::default();
                }
            }
        } else {
            self.host_panel_area = Rect::default();
            self.host_info_area = Rect::default();
        }

        if !self.tabs.is_empty() {
            self.render_tabs(frame, main_chunks[1]);
        } else {
            self.render_host_details(frame, main_chunks[1]);
        }

        if show_host_panel && main_chunks[1].width > 0 {
            let divider_style = if self.is_dragging_divider {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            draw_vertical_rule(
                frame,
                self.host_panel_area.x + self.host_panel_area.width.saturating_sub(1),
                content_area.y,
                content_area.height,
                divider_style,
            );
        }

        draw_horizontal_rule(
            frame,
            separator_area.y,
            separator_area.x,
            separator_area.width,
            Style::default().fg(Color::DarkGray),
        );

        self.render_global_status_bar(frame, status_area);
        self.render_quick_connect_modal(frame, size);
    }

    /// Render tab bar + active tab content.
    fn render_tabs(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        self.tab_bar_area = chunks[0];
        self.render_tab_bar(frame, chunks[0]);

        if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            self.render_tab_content(frame, chunks[1], self.selected_tab);
        }
    }

    /// Render the tab strip.
    fn render_tab_bar(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let tab_widths: Vec<usize> = self.tabs.iter().enumerate().map(|(idx, _)| self.tab_display_width(idx)).collect();
        let available_width = area.width as usize;
        self.tab_scroll_offset = self.normalize_tab_scroll_offset(self.tab_scroll_offset, available_width);

        let has_left_overflow = self.prev_tab_scroll_offset(self.tab_scroll_offset, available_width).is_some();
        let left_slot = if has_left_overflow { 1 } else { 0 };
        let has_right_overflow = self.next_tab_scroll_offset(self.tab_scroll_offset, available_width).is_some();
        let right_slot = if has_right_overflow { 1 } else { 0 };
        let visible_tab_width = available_width.saturating_sub(left_slot + right_slot);

        let mut spans: Vec<Span> = Vec::new();
        if has_left_overflow {
            spans.push(Span::styled("◀", Style::default().fg(Color::Cyan)));
        }

        let mut running_start = 0usize;
        let mut first_visible_idx = 0usize;
        while first_visible_idx < self.tabs.len() && running_start + tab_widths[first_visible_idx] <= self.tab_scroll_offset {
            running_start += tab_widths[first_visible_idx];
            first_visible_idx += 1;
        }

        let mut used = 0usize;
        let mut idx = first_visible_idx;
        while idx < self.tabs.len() && used < visible_tab_width {
            let tab = &self.tabs[idx];
            let is_selected = idx == self.selected_tab && !self.focus_on_manager;
            let style = if is_selected {
                Style::default().fg(Color::Yellow).bg(Color::Indexed(238)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray).bg(Color::Indexed(236))
            };
            let close_style = if is_selected {
                Style::default().fg(Color::LightRed).bg(Color::Indexed(238)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red).bg(Color::Indexed(236)).add_modifier(Modifier::BOLD)
            };

            let mut push_clipped = |text: &str, text_style: Style| {
                if used >= visible_tab_width {
                    return;
                }
                let remaining = visible_tab_width - used;
                let chunk = truncate_to_display_width(text, remaining);
                if !chunk.is_empty() {
                    let width = display_width(&chunk);
                    spans.push(Span::styled(chunk, text_style));
                    used += width;
                }
            };

            push_clipped(&format!("{} ", tab.title), style);
            push_clipped("×", close_style);
            push_clipped(" ", Style::default().fg(Color::DarkGray));
            idx += 1;
        }

        let remaining = visible_tab_width.saturating_sub(used);
        if remaining > 0 {
            spans.push(Span::raw(" ".repeat(remaining)));
        }

        if has_right_overflow {
            spans.push(Span::styled("▶", Style::default().fg(Color::Cyan)));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// Render active tab terminal content.
    fn render_tab_content(&mut self, frame: &mut Frame, area: Rect, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }

        self.resize_current_pty(area);
        self.tab_content_area = area;

        let tab = &self.tabs[tab_idx];
        let host = &tab.host;
        let scroll_offset = tab.scroll_offset;
        let sel_start = self.selection_start;
        let sel_end = self.selection_end;
        let (search_row_ranges, current_search_range) = build_search_row_ranges(self.current_tab_search());

        let session_active = tab.session.is_some();

        if session_active {
            let tab = &self.tabs[tab_idx];
            if let Some(session) = &tab.session
                && let Ok(mut parser) = session.parser.lock()
            {
                parser.set_scrollback(scroll_offset);
                let screen = parser.screen();
                let (vt_rows, vt_cols) = screen.size();
                let cursor_position = screen.cursor_position();
                let hide_cursor = screen.hide_cursor();

                let buf = frame.buffer_mut();

                let render_rows = area.height.min(vt_rows);
                let render_cols = area.width.min(vt_cols);

                for row in 0..render_rows {
                    for col in 0..render_cols {
                        let cell = match screen.cell(row, col) {
                            Some(cell) => cell,
                            None => continue,
                        };

                        let ch = if cell.has_contents() { cell.contents() } else { " ".to_string() };

                        let is_cursor = !hide_cursor && scroll_offset == 0 && row == cursor_position.0 && col == cursor_position.1;
                        let abs_row = row as i64 - scroll_offset as i64;
                        let is_selected = is_cell_in_selection(abs_row, col, sel_start, sel_end);

                        let is_search_match = search_row_ranges
                            .get(&abs_row)
                            .is_some_and(|ranges| ranges.iter().any(|(start_col, end_col)| col >= *start_col && col < *end_col));
                        let is_current_search_match = current_search_range
                            .as_ref()
                            .is_some_and(|(match_row, start_col, end_col)| abs_row == *match_row && col >= *start_col && col < *end_col);

                        let style = if is_current_search_match {
                            let mut s = Style::default().bg(Color::Yellow).fg(Color::Black);
                            if cell.bold() {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        } else if is_search_match {
                            let mut s = Style::default().bg(Color::DarkGray).fg(Color::Yellow);
                            if cell.bold() {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        } else if is_selected {
                            let mut s = Style::default().bg(Color::Blue).fg(Color::White);
                            if cell.bold() {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        } else if is_cursor {
                            let mut s = Style::default().bg(Color::White).fg(Color::Black);
                            if cell.bold() {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        } else {
                            let mut fg_color = vt100_to_ratatui_color(cell.fgcolor());
                            let mut bg_color = vt100_to_ratatui_color(cell.bgcolor());

                            if cell.inverse() {
                                std::mem::swap(&mut fg_color, &mut bg_color);
                                if fg_color == Color::Reset {
                                    fg_color = Color::Black;
                                }
                                if bg_color == Color::Reset {
                                    bg_color = Color::White;
                                }
                            }

                            let mut s = Style::default();

                            if fg_color != Color::Reset {
                                s = s.fg(fg_color);
                            }
                            if bg_color != Color::Reset {
                                s = s.bg(bg_color);
                            }
                            if cell.bold() {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            if cell.italic() {
                                s = s.add_modifier(Modifier::ITALIC);
                            }
                            if cell.underline() {
                                s = s.add_modifier(Modifier::UNDERLINED);
                            }
                            s
                        };

                        let buf_x = area.x + col;
                        let buf_y = area.y + row;

                        if buf_x < area.x + area.width && buf_y < area.y + area.height {
                            let buf_cell = &mut buf[(buf_x, buf_y)];
                            buf_cell.set_symbol(&ch);
                            buf_cell.set_style(style);
                        }
                    }
                }
            }
        } else {
            let error_lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Failed to start SSH session for ", Style::default().fg(Color::Red)),
                    Span::styled(&host.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::Gray)),
                    Span::styled("Ctrl+W", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(" to close this tab", Style::default().fg(Color::Gray)),
                ]),
            ];

            let paragraph = Paragraph::new(error_lines).style(Style::default().fg(Color::Red));
            frame.render_widget(paragraph, area);
        }
    }
}
