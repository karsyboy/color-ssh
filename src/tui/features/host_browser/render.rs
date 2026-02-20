//! Host browser rendering.

use crate::tui::ui::theme;
use crate::tui::{HostTreeRowKind, SessionManager};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

impl SessionManager {
    // Host list panel.
    pub(crate) fn render_host_list(&mut self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            self.host_list_area = Rect::default();
            return;
        }

        let header_area = Rect::new(area.x, area.y, area.width, 1);
        let list_area = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));

        self.host_list_area = list_area;
        let viewport_height = list_area.height as usize;

        self.update_host_scroll(viewport_height.max(1));

        let total_hosts = self.hosts.len();
        let matched_hosts = self.matched_host_count();
        let title = if self.search_query.is_empty() {
            format!("Hosts - {}", total_hosts)
        } else {
            format!("Hosts - {}/{}", matched_hosts, total_hosts)
        };

        let title_style = if self.focus_on_manager {
            Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(title, title_style))), header_area);

        if list_area.height == 0 {
            return;
        }

        let visible_hosts: Vec<ListItem> = self
            .visible_host_rows
            .iter()
            .skip(self.host_scroll_offset)
            .take(viewport_height)
            .map(|row| match row.kind {
                HostTreeRowKind::Folder(_) => {
                    let glyph = if row.expanded { "▾" } else { "▸" };
                    let indent = "  ".repeat(row.depth);
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(glyph, Style::default().fg(theme::ansi_cyan())),
                        Span::raw(" "),
                        Span::styled(row.display_name.clone(), Style::default().fg(theme::ansi_bright_cyan())),
                    ]))
                }
                HostTreeRowKind::Host(_) => {
                    let indent = "  ".repeat(row.depth);
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(row.display_name.clone(), Style::default().fg(theme::ansi_bright_white())),
                    ]))
                }
            })
            .collect();

        let list = List::new(visible_hosts).highlight_style(Style::default().bg(theme::ansi_bright_black()).add_modifier(Modifier::BOLD));

        let adjusted_selection = self.selected_host_row.saturating_sub(self.host_scroll_offset);
        let mut adjusted_state = ListState::default();
        adjusted_state.select(Some(adjusted_selection));
        frame.render_stateful_widget(list, list_area, &mut adjusted_state);

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
                        cell.set_style(Style::default().fg(theme::ansi_cyan()));
                    } else {
                        let cell = &mut frame.buffer_mut()[(scrollbar_x, row_y)];
                        cell.set_symbol("│");
                        cell.set_style(Style::default().fg(theme::ansi_bright_black()));
                    }
                }
            }
        }
    }

    // Compact info panel.
    pub(crate) fn render_host_info(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }

        let header_area = Rect::new(area.x, area.y, area.width, 1);
        let body_area = Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));

        let header_style = if self.focus_on_manager {
            Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };

        if self.visible_host_rows.is_empty() {
            frame.render_widget(Paragraph::new(Line::from(Span::styled("Info", header_style))), header_area);
            frame.render_widget(Paragraph::new("No selection").style(Style::default().fg(theme::ansi_bright_black())), body_area);
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

            if let Some(desc) = &host.description {
                lines.push(Line::from(vec![Span::styled(
                    desc,
                    Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::ITALIC),
                )]));
                lines.push(Line::from(""));
            }

            if let Some(hostname) = &host.hostname {
                lines.push(Line::from(vec![
                    Span::styled("Host: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(hostname, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(user) = &host.user {
                lines.push(Line::from(vec![
                    Span::styled("User: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(user, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled("Port: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(port.to_string(), Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                let display = identity.rsplit('/').next().unwrap_or(identity);
                lines.push(Line::from(vec![
                    Span::styled("Key:  ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(display, Style::default().fg(theme::ansi_bright_black())),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled("Jump: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(proxy, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            for fwd in &host.local_forward {
                lines.push(Line::from(vec![
                    Span::styled("LFwd: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(fwd, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }
            for fwd in &host.remote_forward {
                lines.push(Line::from(vec![
                    Span::styled("RFwd: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(fwd, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(profile) = &host.profile {
                lines.push(Line::from(vec![
                    Span::styled("Prof: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(profile, Style::default().fg(theme::ansi_magenta())),
                ]));
            }

            if host.use_sshpass {
                lines.push(Line::from(vec![
                    Span::styled("Pass: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled("sshpass", Style::default().fg(theme::ansi_yellow())),
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
                    Span::styled("Path: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(folder.path.display().to_string(), Style::default().fg(theme::ansi_bright_white())),
                ]),
                Line::from(vec![
                    Span::styled("Folders: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(folder.children.len().to_string(), Style::default().fg(theme::ansi_bright_white())),
                ]),
                Line::from(vec![
                    Span::styled("Hosts: ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled(
                        format!("{} direct / {} total", folder.host_indices.len(), total_hosts),
                        Style::default().fg(theme::ansi_bright_white()),
                    ),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), body_area);
            return;
        }

        frame.render_widget(Paragraph::new(Line::from(Span::styled("Info", header_style))), header_area);
        frame.render_widget(Paragraph::new("No selection").style(Style::default().fg(theme::ansi_bright_black())), body_area);
    }

    // Detailed host/folder panel (shown when no tabs are open).
    pub(crate) fn render_host_details(&self, frame: &mut Frame, area: Rect) {
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
                    Span::styled("Host: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(&host.name, Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
            ];

            if let Some(hostname) = &host.hostname {
                lines.push(Line::from(vec![
                    Span::styled("  Hostname: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(hostname, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(user) = &host.user {
                lines.push(Line::from(vec![
                    Span::styled("  User: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(user, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled("  Port: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(port.to_string(), Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                lines.push(Line::from(vec![
                    Span::styled("  IdentityFile: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(identity, Style::default().fg(theme::ansi_bright_black())),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled("  ProxyJump: ", Style::default().fg(theme::ansi_white())),
                    Span::styled(proxy, Style::default().fg(theme::ansi_bright_white())),
                ]));
            }

            lines
        } else if let Some(folder_id) = self.selected_folder_id() {
            if let Some(folder) = self.folder_by_id(folder_id) {
                vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Folder: ", Style::default().fg(theme::ansi_white())),
                        Span::styled(&folder.name, Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  Path: ", Style::default().fg(theme::ansi_white())),
                        Span::styled(folder.path.display().to_string(), Style::default().fg(theme::ansi_bright_white())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Child Folders: ", Style::default().fg(theme::ansi_white())),
                        Span::styled(folder.children.len().to_string(), Style::default().fg(theme::ansi_bright_white())),
                    ]),
                    Line::from(vec![
                        Span::styled("  Hosts: ", Style::default().fg(theme::ansi_white())),
                        Span::styled(
                            format!("{} direct / {} total", folder.host_indices.len(), self.folder_descendant_host_count(folder_id)),
                            Style::default().fg(theme::ansi_bright_white()),
                        ),
                    ]),
                ]
            } else {
                vec![
                    Line::from(""),
                    Line::from(Span::styled("No folder selected", Style::default().fg(theme::ansi_bright_black()))),
                ]
            }
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("No selection", Style::default().fg(theme::ansi_bright_black()))),
            ]
        };

        let header = Paragraph::new(Line::from(Span::styled(
            "Host Details",
            Style::default().fg(theme::ansi_bright_black()).add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(header, header_area);
        frame.render_widget(Paragraph::new(content), body_area);
    }
}
