//! UI rendering for the session manager

use super::SessionManager;
use super::selection::is_cell_in_selection;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use std::collections::HashMap;

type SearchRowRanges = HashMap<i64, Vec<(u16, u16)>>;
type CurrentSearchRange = Option<(i64, u16, u16)>;

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

fn build_search_row_ranges(search: Option<&super::TerminalSearchState>) -> (SearchRowRanges, CurrentSearchRange) {
    let Some(search_state) = search else {
        return (HashMap::new(), None);
    };
    if !search_state.active || search_state.matches.is_empty() {
        return (HashMap::new(), None);
    }

    let current_match = search_state.matches.get(search_state.current).map(|(row, col, len)| {
        let end_col = col.saturating_add(*len as u16);
        (*row, *col, end_col)
    });

    let mut row_ranges: HashMap<i64, Vec<(u16, u16)>> = HashMap::new();
    for (row, col, len) in &search_state.matches {
        let end_col = col.saturating_add(*len as u16);
        row_ranges.entry(*row).or_default().push((*col, end_col));
    }

    (row_ranges, current_match)
}

impl SessionManager {
    /// Render the UI
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();
        let root_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1), Constraint::Length(1)])
            .split(size);
        let content_area = root_chunks[0];
        let separator_area = root_chunks[1];
        let status_area = root_chunks[2];

        // Create main layout: adjustable left panel and expanding right panel (or full width if hidden)
        let (main_chunks, show_host_panel) = if self.host_panel_visible {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(self.host_panel_width), Constraint::Min(0)])
                .split(content_area);
            (chunks, true)
        } else {
            // Host panel hidden, use full width for content
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(0), Constraint::Min(0)])
                .split(content_area);
            (chunks, false)
        };

        if show_host_panel {
            let host_panel_area = main_chunks[0];
            // Reserve one column for the visual divider so terminal text is never overdrawn.
            let host_content_area = Rect::new(
                host_panel_area.x,
                host_panel_area.y,
                host_panel_area.width.saturating_sub(1),
                host_panel_area.height,
            );

            // Cache the full host panel area for click-to-focus
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

                // Render host list
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
            // Clear the cached area when hidden
            self.host_panel_area = Rect::default();
            self.host_info_area = Rect::default();
        }

        // If there are tabs, render tabs; otherwise render help panel
        if !self.tabs.is_empty() {
            self.render_tabs(frame, main_chunks[1]);
        } else {
            self.render_host_details(frame, main_chunks[1]);
        }

        if show_host_panel && main_chunks[1].width > 0 {
            // Draw a subtle vertical divider between host and terminal panes.
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

        // Draw a subtle divider above the status bar.
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

    fn render_quick_connect_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(form) = &self.quick_connect else {
            return;
        };

        let width = full_area.width.clamp(44, 74);
        let height = if form.error.is_some() { 12 } else { 11 };
        let area = Self::centered_rect(width, height, full_area);

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Quick Connect ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let selected_label = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let normal_label = Style::default().fg(Color::DarkGray);
        let selected_value = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
        let normal_value = Style::default().fg(Color::White);

        let field_style = |field, selected: super::QuickConnectField| {
            if field == selected { selected_label } else { normal_label }
        };
        let value_style = |field, selected: super::QuickConnectField| {
            if field == selected { selected_value } else { normal_value }
        };

        let user_text = if form.selected == super::QuickConnectField::User {
            format!("{}_", form.user)
        } else {
            form.user.clone()
        };
        let host_text = if form.selected == super::QuickConnectField::Host {
            format!("{}_", form.host)
        } else {
            form.host.clone()
        };
        let profile_text = form.selected_profile_label().to_string();
        let profile_text = if form.selected == super::QuickConnectField::Profile {
            format!("{}_", profile_text)
        } else {
            profile_text
        };
        let mut profile_list_spans = vec![Span::styled("Profiles: ", Style::default().fg(Color::DarkGray))];
        for (idx, profile_name) in form.profile_options.iter().enumerate() {
            if idx > 0 {
                profile_list_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            }
            let style = if idx == form.profile_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            profile_list_spans.push(Span::styled(profile_name.clone(), style));
        }

        let logging_mark = if form.ssh_logging { "[x]" } else { "[ ]" };
        let connect_style = if form.selected == super::QuickConnectField::Connect {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("User: ", field_style(super::QuickConnectField::User, form.selected)),
                Span::styled(
                    if user_text.is_empty() { "(optional)".to_string() } else { user_text },
                    value_style(super::QuickConnectField::User, form.selected),
                ),
            ]),
            Line::from(vec![
                Span::styled("Host: ", field_style(super::QuickConnectField::Host, form.selected)),
                Span::styled(host_text, value_style(super::QuickConnectField::Host, form.selected)),
            ]),
            Line::from(vec![
                Span::styled("Profile: ", field_style(super::QuickConnectField::Profile, form.selected)),
                Span::styled(profile_text, value_style(super::QuickConnectField::Profile, form.selected)),
            ]),
            Line::from(profile_list_spans),
            Line::from(vec![
                Span::styled("SSH Logging: ", field_style(super::QuickConnectField::Logging, form.selected)),
                Span::styled(format!("{} (-l)", logging_mark), value_style(super::QuickConnectField::Logging, form.selected)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled("[ Enter ] Connect", connect_style)]),
            Line::from(vec![Span::styled(
                "Esc: cancel | Tab/Shift+Tab: field | \u{2190}/\u{2192}: profile",
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        if let Some(error) = &form.error {
            lines.push(Line::from(vec![Span::styled(error.clone(), Style::default().fg(Color::Red))]));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
        let popup_width = width.min(area.width);
        let popup_height = height.min(area.height);
        let popup_x = area.x + area.width.saturating_sub(popup_width) / 2;
        let popup_y = area.y + area.height.saturating_sub(popup_height) / 2;
        Rect::new(popup_x, popup_y, popup_width, popup_height)
    }

    /// Render the host list
    fn render_host_list(&mut self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            self.host_list_area = Rect::default();
            return;
        }

        let header_area = Rect::new(area.x, area.y, area.width, 1);
        let list_area = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));

        // Cache only the selectable list rows (no decorative header).
        self.host_list_area = list_area;
        let viewport_height = list_area.height as usize;

        // Update scroll to keep selection visible
        self.update_host_scroll(viewport_height.max(1));

        let total_hosts = self.hosts.len();
        let matched_hosts = self.matched_host_count();
        let title = if self.search_query.is_empty() {
            format!("Hosts - {}", total_hosts)
        } else {
            format!("Hosts - {}/{}", matched_hosts, total_hosts)
        };

        let title_style = if self.focus_on_manager {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(title, title_style))), header_area);

        if list_area.height == 0 {
            return;
        }

        // Create visible tree rows with scrolling.
        let visible_hosts: Vec<ListItem> = self
            .visible_host_rows
            .iter()
            .skip(self.host_scroll_offset)
            .take(viewport_height)
            .map(|row| match row.kind {
                super::HostTreeRowKind::Folder(_) => {
                    let glyph = if row.expanded { "▾" } else { "▸" };
                    let indent = "  ".repeat(row.depth);
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(glyph, Style::default().fg(Color::Cyan)),
                        Span::raw(" "),
                        Span::styled(row.display_name.clone(), Style::default().fg(Color::LightCyan)),
                    ]))
                }
                super::HostTreeRowKind::Host(_) => {
                    let indent = "  ".repeat(row.depth);
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(row.display_name.clone(), Style::default().fg(Color::White)),
                    ]))
                }
            })
            .collect();

        let list = List::new(visible_hosts).highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

        // Adjust list state for scrolling
        let adjusted_selection = self.selected_host_row.saturating_sub(self.host_scroll_offset);
        let mut adjusted_state = ListState::default();
        adjusted_state.select(Some(adjusted_selection));
        frame.render_stateful_widget(list, list_area, &mut adjusted_state);

        // Draw scrollbar if needed
        let total_rows = self.visible_host_rows.len();
        if total_rows > viewport_height && list_area.width > 0 {
            let scrollbar_height = list_area.height as usize;
            if scrollbar_height > 0 {
                let thumb_size = (scrollbar_height * viewport_height / total_rows).max(1);
                let thumb_position = (scrollbar_height * self.host_scroll_offset / total_rows).min(scrollbar_height.saturating_sub(thumb_size));
                let scrollbar_x = list_area.x + list_area.width - 1;

                for scrollbar_row_idx in 0..scrollbar_height {
                    let row_y = list_area.y + scrollbar_row_idx as u16;
                    if scrollbar_row_idx >= thumb_position && scrollbar_row_idx < thumb_position + thumb_size {
                        let cell = &mut frame.buffer_mut()[(scrollbar_x, row_y)];
                        cell.set_symbol("█");
                        cell.set_style(Style::default().fg(Color::Cyan));
                    } else {
                        let cell = &mut frame.buffer_mut()[(scrollbar_x, row_y)];
                        cell.set_symbol("│");
                        cell.set_style(Style::default().fg(Color::DarkGray));
                    }
                }
            }
        }
    }

    /// Render the host info panel below the host list on the left side
    fn render_host_info(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        let header_area = Rect::new(area.x, area.y, area.width, 1);
        let body_area = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));

        let header_style = if self.focus_on_manager {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if self.visible_host_rows.is_empty() {
            frame.render_widget(Paragraph::new(Line::from(Span::styled("Info", header_style))), header_area);
            frame.render_widget(Paragraph::new("No selection").style(Style::default().fg(Color::DarkGray)), body_area);
            return;
        }

        if let Some(host_idx) = self.selected_host_idx()
            && let Some(host) = self.hosts.get(host_idx)
        {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("Info {}", host.name), header_style))),
                header_area,
            );

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

            let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
            frame.render_widget(paragraph, body_area);
            return;
        }

        if let Some(folder_id) = self.selected_folder_id()
            && let Some(folder) = self.folder_by_id(folder_id)
        {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("Info {}", folder.name), header_style))),
                header_area,
            );
            let total_hosts = self.folder_descendant_host_count(folder_id);
            let lines = vec![
                Line::from(vec![
                    Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(folder.path.display().to_string(), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Folders: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(folder.children.len().to_string(), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Hosts: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{} direct / {} total", folder.host_indices.len(), total_hosts),
                        Style::default().fg(Color::White),
                    ),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), body_area);
            return;
        }

        frame.render_widget(Paragraph::new(Line::from(Span::styled("Info", header_style))), header_area);
        frame.render_widget(Paragraph::new("No selection").style(Style::default().fg(Color::DarkGray)), body_area);
    }

    /// Render the host details panel (shown when no tabs are open)
    fn render_host_details(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        let header_area = Rect::new(area.x, area.y, area.width, 1);
        let body_area = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));

        let content = if let Some(host_idx) = self.selected_host_idx() {
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

            lines
        } else if let Some(folder_id) = self.selected_folder_id() {
            if let Some(folder) = self.folder_by_id(folder_id) {
                vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Folder: ", Style::default().fg(Color::Gray)),
                        Span::styled(&folder.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  Path: ", Style::default().fg(Color::Gray)),
                        Span::styled(folder.path.display().to_string(), Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("  Child Folders: ", Style::default().fg(Color::Gray)),
                        Span::styled(folder.children.len().to_string(), Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled("  Hosts: ", Style::default().fg(Color::Gray)),
                        Span::styled(
                            format!("{} direct / {} total", folder.host_indices.len(), self.folder_descendant_host_count(folder_id)),
                            Style::default().fg(Color::White),
                        ),
                    ]),
                ]
            } else {
                vec![
                    Line::from(""),
                    Line::from(Span::styled("No folder selected", Style::default().fg(Color::DarkGray))),
                ]
            }
        } else {
            vec![Line::from(""), Line::from(Span::styled("No selection", Style::default().fg(Color::DarkGray)))]
        };

        let header = Paragraph::new(Line::from(Span::styled(
            "Host Details",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(header, header_area);
        frame.render_widget(Paragraph::new(content), body_area);
    }

    /// Render the tabs panel (tab bar + active tab content)
    fn render_tabs(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area vertically: tab bar at top, content below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
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
        if area.width == 0 || area.height == 0 {
            return;
        }
        let tab_widths: Vec<usize> = self.tabs.iter().map(|tab| tab.title.len() + 3).collect();
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

        // Skip tabs scrolled out on the left.
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
                let chunk: String = text.chars().take(remaining).collect();
                if !chunk.is_empty() {
                    let width = chunk.chars().count();
                    spans.push(Span::styled(chunk, text_style));
                    used += width;
                }
            };

            push_clipped(&format!("{} ", tab.title), style);
            push_clipped("×", close_style);
            push_clipped(" ", Style::default().fg(Color::DarkGray));
            idx += 1;
        }

        // Keep overflow indicator hitbox and glyph aligned by placing ▶ in the final
        // tab-bar column (the click logic expects last-column placement).
        let remaining = visible_tab_width.saturating_sub(used);
        if remaining > 0 {
            spans.push(Span::raw(" ".repeat(remaining)));
        }

        if has_right_overflow {
            spans.push(Span::styled("▶", Style::default().fg(Color::Cyan)));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// Render the content of a specific tab
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

        // Check if session exists
        let session_active = tab.session.is_some();

        if session_active {
            // Now render VT100 screen directly into the buffer cell-by-cell
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

            let paragraph = Paragraph::new(error_lines).style(Style::default().fg(Color::Red));

            frame.render_widget(paragraph, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_search_row_ranges;
    use crate::tui::TerminalSearchState;

    #[test]
    fn build_search_row_ranges_groups_matches_by_row() {
        let search = TerminalSearchState {
            active: true,
            query: "abc".to_string(),
            matches: vec![(2, 4, 3), (2, 10, 2), (3, 1, 1)],
            current: 1,
        };

        let (ranges, current) = build_search_row_ranges(Some(&search));
        assert_eq!(ranges.get(&2).map(Vec::len), Some(2));
        assert_eq!(ranges.get(&3).map(Vec::len), Some(1));
        assert_eq!(current, Some((2, 10, 12)));
    }
}
