//! UI rendering for the session manager

use super::App;
use super::selection::is_cell_in_selection;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

/// Convert VT100 color to Ratatui color
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

impl App {
    /// Render the UI
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();

        // Create main layout: adjustable left panel and expanding right panel (or full width if hidden)
        let (main_chunks, show_host_panel) = if self.host_panel_visible {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(self.host_panel_width), Constraint::Min(0)])
                .split(size);
            (chunks, true)
        } else {
            // Host panel hidden, use full width for content
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(0), Constraint::Min(0)])
                .split(size);
            (chunks, false)
        };

        if show_host_panel {
            // Split the left panel: host list on top, host info on bottom
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(main_chunks[0]);

            // Cache the full host panel area for click-to-focus
            self.host_panel_area = main_chunks[0];

            // Render host list
            self.render_host_list(frame, left_chunks[0]);

            // Render host info panel below the list
            self.render_host_info(frame, left_chunks[1]);
        } else {
            // Clear the cached area when hidden
            self.host_panel_area = Rect::default();
        }

        // Cache exit button area: the X sits in the top-right border of the right panel
        // " X " is 3 chars, positioned inside the top border of main_chunks[1]
        let right_area = main_chunks[1];
        if right_area.width > 4 {
            // The X character is at: right_area.x + right_area.width - 3 (accounting for border + " X ")
            self.exit_button_area = Rect::new(right_area.x + right_area.width - 4, right_area.y, 3, 1);
        }

        // If there are tabs, render tabs; otherwise render help panel
        if !self.tabs.is_empty() {
            self.render_tabs(frame, main_chunks[1]);
        } else {
            self.render_host_details(frame, main_chunks[1]);
        }
    }

    /// Render the host list
    fn render_host_list(&mut self, frame: &mut Frame, area: Rect) {
        // Cache area and calculate viewport
        self.host_list_area = area;
        let viewport_height = area.height.saturating_sub(3) as usize; // minus borders and title

        // Update scroll to keep selection visible
        self.update_host_scroll(viewport_height);

        // Create visible items with scrolling
        let visible_hosts: Vec<ListItem> = self
            .filtered_hosts
            .iter()
            .skip(self.host_scroll_offset)
            .take(viewport_height)
            .map(|(idx, _score)| {
                let host = &self.hosts[*idx];

                ListItem::new(host.name.clone())
            })
            .collect();

        let title = if self.search_mode {
            format!(" SSH Hosts (Search: {}_) ", self.search_query)
        } else {
            let total = self.filtered_hosts.len();
            let showing = visible_hosts.len();
            let tabs_info = if !self.tabs.is_empty() {
                format!(" | {} tabs", self.tabs.len())
            } else {
                String::new()
            };

            if self.host_scroll_offset > 0 || showing < total {
                format!(" Hosts ({}/{}){}  [Ctrl+←/→: resize] ", showing, total, tabs_info)
            } else {
                format!(" Hosts ({}){}  [Ctrl+←/→: resize] ", total, tabs_info)
            }
        };

        let border_style = if self.focus_on_manager {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let list = List::new(visible_hosts)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

        // Adjust list state for scrolling
        let adjusted_selection = self.selected_host.saturating_sub(self.host_scroll_offset);
        let mut adjusted_state = ListState::default();
        adjusted_state.select(Some(adjusted_selection));

        frame.render_stateful_widget(list, area, &mut adjusted_state);
        
        // Draw scrollbar if needed
        if self.filtered_hosts.len() > viewport_height {
            let scrollbar_height = area.height.saturating_sub(2) as usize; // minus borders
            if scrollbar_height > 0 {
                let total_items = self.filtered_hosts.len();
                let thumb_size = (scrollbar_height * viewport_height / total_items).max(1);
                let thumb_position = (scrollbar_height * self.host_scroll_offset / total_items).min(scrollbar_height.saturating_sub(thumb_size));
                
                let scrollbar_x = area.x + area.width - 1; // Right border position
                
                // Draw the scrollbar
                for i in 0..scrollbar_height {
                    let y = area.y + 1 + i as u16; // +1 for top border
                    if i >= thumb_position && i < thumb_position + thumb_size {
                        // Scrollbar thumb
                        let cell = &mut frame.buffer_mut()[(scrollbar_x, y)];
                        cell.set_symbol("█");
                        cell.set_style(Style::default().fg(Color::Cyan));
                    } else {
                        // Scrollbar track
                        let cell = &mut frame.buffer_mut()[(scrollbar_x, y)];
                        cell.set_symbol("│");
                        cell.set_style(Style::default().fg(Color::DarkGray));
                    }
                }
            }
        }
    }

    /// Render the host info panel below the host list on the left side
    fn render_host_info(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.focus_on_manager {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if self.filtered_hosts.is_empty() {
            let paragraph = Paragraph::new("No host selected")
                .block(Block::default().title(" Info ").borders(Borders::ALL).border_style(border_style))
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(paragraph, area);
            return;
        }

        let host_idx = self.filtered_hosts[self.selected_host].0;
        let host = &self.hosts[host_idx];

        let mut lines = Vec::new();

        // Description
        if let Some(desc) = &host.description {
            lines.push(Line::from(vec![Span::styled(
                desc,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC),
            )]));
            lines.push(Line::from(""));
        }

        // Hostname
        if let Some(hostname) = &host.hostname {
            lines.push(Line::from(vec![
                Span::styled("Host: ", Style::default().fg(Color::DarkGray)),
                Span::styled(hostname, Style::default().fg(Color::White)),
            ]));
        }

        // User
        if let Some(user) = &host.user {
            lines.push(Line::from(vec![
                Span::styled("User: ", Style::default().fg(Color::DarkGray)),
                Span::styled(user, Style::default().fg(Color::White)),
            ]));
        }

        // Port
        if let Some(port) = &host.port {
            lines.push(Line::from(vec![
                Span::styled("Port: ", Style::default().fg(Color::DarkGray)),
                Span::styled(port.to_string(), Style::default().fg(Color::White)),
            ]));
        }

        // Identity file (show just filename)
        if let Some(identity) = &host.identity_file {
            let display = identity.rsplit('/').next().unwrap_or(identity);
            lines.push(Line::from(vec![
                Span::styled("Key:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(display, Style::default().fg(Color::DarkGray)),
            ]));
        }

        // ProxyJump
        if let Some(proxy) = &host.proxy_jump {
            lines.push(Line::from(vec![
                Span::styled("Jump: ", Style::default().fg(Color::DarkGray)),
                Span::styled(proxy, Style::default().fg(Color::White)),
            ]));
        }

        // Forwards
        for fwd in &host.local_forward {
            lines.push(Line::from(vec![
                Span::styled("LFwd: ", Style::default().fg(Color::DarkGray)),
                Span::styled(fwd, Style::default().fg(Color::White)),
            ]));
        }
        for fwd in &host.remote_forward {
            lines.push(Line::from(vec![
                Span::styled("RFwd: ", Style::default().fg(Color::DarkGray)),
                Span::styled(fwd, Style::default().fg(Color::White)),
            ]));
        }

        // Profile
        if let Some(profile) = &host.profile {
            lines.push(Line::from(vec![
                Span::styled("Prof: ", Style::default().fg(Color::DarkGray)),
                Span::styled(profile, Style::default().fg(Color::Magenta)),
            ]));
        }

        // SSHPass
        if host.use_sshpass {
            lines.push(Line::from(vec![
                Span::styled("Pass: ", Style::default().fg(Color::DarkGray)),
                Span::styled("sshpass", Style::default().fg(Color::Yellow)),
            ]));
        }

        let title = format!(" {} ", host.name);
        let paragraph = Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render the host details panel (shown when no tabs are open)
    fn render_host_details(&self, frame: &mut Frame, area: Rect) {
        let content = if !self.filtered_hosts.is_empty() {
            let host_idx = self.filtered_hosts[self.selected_host].0;
            let host = &self.hosts[host_idx];

            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Host: ", Style::default().fg(Color::Gray)),
                    Span::styled(&host.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
            ];

            if let Some(hostname) = &host.hostname {
                lines.push(Line::from(vec![
                    Span::styled("  Hostname: ", Style::default().fg(Color::Gray)),
                    Span::styled(hostname, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(user) = &host.user {
                lines.push(Line::from(vec![
                    Span::styled("  User: ", Style::default().fg(Color::Gray)),
                    Span::styled(user, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled("  Port: ", Style::default().fg(Color::Gray)),
                    Span::styled(port.to_string(), Style::default().fg(Color::White)),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                lines.push(Line::from(vec![
                    Span::styled("  IdentityFile: ", Style::default().fg(Color::Gray)),
                    Span::styled(identity, Style::default().fg(Color::DarkGray)),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled("  ProxyJump: ", Style::default().fg(Color::Gray)),
                    Span::styled(proxy, Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(" to open in new tab", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Shift+Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" to switch focus to tabs", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled(" to quit", Style::default().fg(Color::Gray)),
            ]));

            lines
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("No hosts found", Style::default().fg(Color::DarkGray))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::Gray)),
                    Span::styled("/", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(" to search", Style::default().fg(Color::Gray)),
                ]),
            ]
        };

        let exit_title = Line::from(vec![Span::styled(" X ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))]);

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .title(" Host Details ")
                .title_top(exit_title.alignment(Alignment::Right))
                .borders(Borders::ALL),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render the tabs panel (tab bar + active tab content)
    fn render_tabs(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area vertically: tab bar at top, content below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Cache tab bar area and render
        self.tab_bar_area = chunks[0];
        self.render_tab_bar(frame, chunks[0]);

        // Render current tab content
        if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            self.render_tab_content(frame, chunks[1], self.selected_tab);
        }
    }

    /// Render the tab bar showing all open tabs
    fn render_tab_bar(&mut self, frame: &mut Frame, area: Rect) {
        // Build full tab title spans first, then apply horizontal scroll offset
        let mut all_spans: Vec<Span> = Vec::new();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_selected = idx == self.selected_tab && !self.focus_on_manager;

            let style = if is_selected {
                Style::default().fg(Color::Yellow).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let close_style = if is_selected {
                Style::default().fg(Color::Red).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red)
            };

            let prefix = if is_selected { " [" } else { " " };
            let suffix = if is_selected { "] " } else { " " };

            all_spans.push(Span::styled(format!("{}{} ", prefix, &tab.title), style));
            all_spans.push(Span::styled("×", close_style));
            all_spans.push(Span::styled(suffix.to_string(), style));

            // Add separator between tabs
            if idx < self.tabs.len() - 1 {
                all_spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            }
        }

        // Calculate total display width of all tab titles (use char count, not byte length,
        // because × and │ are multi-byte but single-column characters)
        let total_width: usize = all_spans.iter().map(|s| s.content.chars().count()).sum();
        let available_width = area.width.saturating_sub(2) as usize; // subtract borders

        // Clamp tab_scroll_offset
        if total_width <= available_width {
            self.tab_scroll_offset = 0;
        } else if self.tab_scroll_offset > total_width.saturating_sub(available_width) {
            self.tab_scroll_offset = total_width.saturating_sub(available_width);
        }

        // Apply horizontal scroll: skip characters from the start
        let mut visible_spans: Vec<Span> = Vec::new();
        let mut skipped = 0usize;
        let scroll_offset = self.tab_scroll_offset;

        // Add scroll indicator if scrolled
        let has_left_overflow = scroll_offset > 0;
        let has_right_overflow = total_width > scroll_offset + available_width;

        for span in &all_spans {
            let span_len = span.content.chars().count();
            if skipped + span_len <= scroll_offset {
                skipped += span_len;
                continue;
            }
            if skipped < scroll_offset {
                // Partial span: skip some leading chars
                let skip_chars = scroll_offset - skipped;
                let trimmed: String = span.content.chars().skip(skip_chars).collect();
                visible_spans.push(Span::styled(trimmed, span.style));
                skipped = scroll_offset;
            } else {
                visible_spans.push(span.clone());
            }
        }

        // Add scroll indicators
        if has_left_overflow {
            visible_spans.insert(0, Span::styled("◀", Style::default().fg(Color::Cyan)));
        }
        if has_right_overflow {
            visible_spans.push(Span::styled("▶", Style::default().fg(Color::Cyan)));
        }

        let border_style = if self.focus_on_manager {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let exit_title = Line::from(vec![Span::styled(" X ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))]);

        let scroll_hint = if self.tabs.len() > 1 { ", Scroll: switch" } else { "" };
        let tabs_line = Line::from(visible_spans);
        let paragraph = Paragraph::new(tabs_line).block(
            Block::default()
                .title(format!(" Tabs [Shift+Tab: switch focus, Alt+←/→: navigate, Ctrl+W: close{}] ", scroll_hint))
                .title_top(exit_title.alignment(Alignment::Right))
                .borders(Borders::ALL)
                .border_style(border_style),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render the content of a specific tab
    fn render_tab_content(&mut self, frame: &mut Frame, area: Rect, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }

        // Resize PTY to match display area - status bar at bottom now
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        self.resize_current_pty(chunks[0]);
        self.tab_content_area = chunks[0];

        let tab = &self.tabs[tab_idx];
        let host = &tab.host;
        let scroll_offset = tab.scroll_offset;
        let sel_start = self.selection_start;
        let sel_end = self.selection_end;
        let tab_title = tab.title.clone();

        // Check if session exists
        let (session_active, is_exited) = if let Some(session) = &tab.session {
            let exited = *session.exited.lock().unwrap();
            (true, exited)
        } else {
            (false, false)
        };

        if session_active {
            // Render the border/block first
            let block = Block::default().borders(Borders::ALL).title(format!(" {} ", &tab_title));
            let inner_area = block.inner(chunks[0]);
            frame.render_widget(block, chunks[0]);

            // Now render VT100 screen directly into the buffer cell-by-cell
            let tab = &self.tabs[tab_idx];
            if let Some(session) = &tab.session {
                if let Ok(mut parser) = session.parser.lock() {
                    parser.set_scrollback(scroll_offset);
                    let screen = parser.screen();
                    let (vt_rows, vt_cols) = screen.size();
                    let cursor_position = screen.cursor_position();
                    let hide_cursor = screen.hide_cursor();

                    let buf = frame.buffer_mut();

                    let render_rows = (inner_area.height as u16).min(vt_rows);
                    let render_cols = (inner_area.width as u16).min(vt_cols);

                    for row in 0..render_rows {
                        for col in 0..render_cols {
                            let cell = match screen.cell(row, col) {
                                Some(c) => c,
                                None => continue,
                            };

                            let ch = if cell.has_contents() {
                                cell.contents()
                            } else {
                                " ".to_string()
                            };

                            let is_cursor = !hide_cursor
                                && scroll_offset == 0
                                && row == cursor_position.0
                                && col == cursor_position.1;
                            let abs_row = row as i64 - scroll_offset as i64;
                            let is_selected = is_cell_in_selection(abs_row, col, sel_start, sel_end);
                            
                            // Check if this cell is part of a search match
                            let is_search_match = self.is_cell_in_search_match(abs_row, col);
                            let is_current_search_match = self.is_cell_in_current_search_match(abs_row, col);

                            // Build the style from VT100 cell attributes
                            let style = if is_current_search_match {
                                // Highlight current search match more prominently
                                let mut s = Style::default().bg(Color::Yellow).fg(Color::Black);
                                if cell.bold() {
                                    s = s.add_modifier(Modifier::BOLD);
                                }
                                s
                            } else if is_search_match {
                                // Highlight other search matches
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

                                // Handle inverse/reverse video
                                if cell.inverse() {
                                    std::mem::swap(&mut fg_color, &mut bg_color);
                                    // If either was default/reset, map to sensible defaults
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

                            let buf_x = inner_area.x + col;
                            let buf_y = inner_area.y + row;

                            if buf_x < inner_area.x + inner_area.width
                                && buf_y < inner_area.y + inner_area.height
                            {
                                let buf_cell = &mut buf[(buf_x, buf_y)];
                                buf_cell.set_symbol(&ch);
                                buf_cell.set_style(style);
                            }
                        }
                    }
                }
            }
            
            // Compact status bar at the bottom
            let scroll_info = if scroll_offset > 0 {
                format!(" [Scrollback: +{}]", scroll_offset)
            } else {
                String::new()
            };
            
            let status_line = if self.terminal_search_mode {
                // Show search box
                let match_info = if !self.terminal_search_matches.is_empty() {
                    format!("({}/{}) ", self.terminal_search_current + 1, self.terminal_search_matches.len())
                } else {
                    "(0) ".to_string()
                };
                vec![
                    Span::styled(" Search: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(self.terminal_search_query.clone(), Style::default().fg(Color::White)),
                    Span::styled("_", Style::default().fg(Color::White)),
                    Span::styled(" ", Style::default()),
                    Span::styled(match_info, Style::default().fg(Color::Yellow)),
                    Span::styled("[", Style::default().fg(Color::DarkGray)),
                    Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
                    Span::styled(": navigate | ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Esc", Style::default().fg(Color::Cyan)),
                    Span::styled(": close]", Style::default().fg(Color::DarkGray)),
                ]
            } else {
                let (status_icon_color, status_text) = if is_exited {
                    (Color::Red, "Disconnected")
                } else {
                    (Color::Green, "Connected")
                };
                
                let mut spans = vec![
                    Span::styled(" ● ", Style::default().fg(status_icon_color).add_modifier(Modifier::BOLD)),
                    Span::styled(status_text, Style::default().fg(Color::White)),
                    Span::styled(": ", Style::default().fg(Color::DarkGray)),
                    Span::styled(host.name.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ];
                
                if !scroll_info.is_empty() {
                    spans.push(Span::styled(scroll_info, Style::default().fg(Color::Yellow)));
                }
                
                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                
                if is_exited {
                    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("Enter", Style::default().fg(Color::Green)));
                    spans.push(Span::styled(": reconnect | ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("Ctrl+W", Style::default().fg(Color::Red)));
                    spans.push(Span::styled(": close]", Style::default().fg(Color::DarkGray)));
                } else {
                    spans.push(Span::styled("[", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("Shift+Tab", Style::default().fg(Color::Cyan)));
                    spans.push(Span::styled(": focus | ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("Ctrl+B", Style::default().fg(Color::Cyan)));
                    spans.push(Span::styled(": toggle panel | ", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled("Ctrl+F", Style::default().fg(Color::Cyan)));
                    spans.push(Span::styled(": search", Style::default().fg(Color::DarkGray)));
                    if scroll_offset > 0 {
                        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                        spans.push(Span::styled("Shift+PgUp/PgDn", Style::default().fg(Color::Yellow)));
                        spans.push(Span::styled(": scroll", Style::default().fg(Color::DarkGray)));
                    }
                    spans.push(Span::styled("]", Style::default().fg(Color::DarkGray)));
                }
                
                spans
            };

            let status = Paragraph::new(Line::from(status_line)).block(Block::default());
            frame.render_widget(status, chunks[1]);
        } else {
            // Session failed to start
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

            let paragraph = Paragraph::new(error_lines).block(
                Block::default()
                    .title(format!(" {} ", &tab_title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );

            frame.render_widget(paragraph, area);
        }
    }

    /// Check if a cell is part of any search match
    fn is_cell_in_search_match(&self, abs_row: i64, col: u16) -> bool {
        if !self.terminal_search_mode || self.terminal_search_matches.is_empty() {
            return false;
        }
        
        for (match_row, match_col, match_len) in &self.terminal_search_matches {
            if *match_row == abs_row && col >= *match_col && (col as usize) < (*match_col as usize + *match_len) {
                return true;
            }
        }
        false
    }

    /// Check if a cell is part of the current search match
    fn is_cell_in_current_search_match(&self, abs_row: i64, col: u16) -> bool {
        if !self.terminal_search_mode || self.terminal_search_matches.is_empty() {
            return false;
        }
        
        let (match_row, match_col, match_len) = self.terminal_search_matches[self.terminal_search_current];
        match_row == abs_row && col >= match_col && (col as usize) < (match_col as usize + match_len)
    }
}
