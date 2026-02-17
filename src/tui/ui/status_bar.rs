//! Global status bar rendering.

use crate::tui::SessionManager;
use crate::tui::ui::theme::display_width;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

#[derive(Debug, Clone, Copy)]
enum StatusContext {
    HostSearch,
    TerminalSearch,
    Host,
    Terminal,
}

impl SessionManager {
    pub(crate) fn render_global_status_bar(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (left_spans, right_spans) = self.build_status_line_sections();
        let base_style = Style::default().fg(Color::Gray);

        if right_spans.is_empty() {
            let status = Paragraph::new(Line::from(left_spans)).style(base_style);
            frame.render_widget(status, area);
            return;
        }

        let right_width = self.spans_display_width(&right_spans).min(area.width as usize) as u16;
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(right_width)])
            .split(area);

        let left = Paragraph::new(Line::from(left_spans)).style(base_style);
        let right = Paragraph::new(Line::from(right_spans)).style(base_style).alignment(Alignment::Right);
        frame.render_widget(left, chunks[0]);
        frame.render_widget(right, chunks[1]);
    }

    fn build_status_line_sections(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        match self.resolve_status_context() {
            StatusContext::HostSearch => self.build_search_mode_status_spans(),
            StatusContext::TerminalSearch => self.build_terminal_search_status_spans(),
            StatusContext::Host => self.build_manager_status_spans(),
            StatusContext::Terminal => self.build_terminal_status_spans(),
        }
    }

    fn spans_display_width(&self, spans: &[Span<'static>]) -> usize {
        spans.iter().map(|span| display_width(span.content.as_ref())).sum()
    }

    fn resolve_status_context(&self) -> StatusContext {
        if self.search_mode {
            return StatusContext::HostSearch;
        }
        if self.has_terminal_focus() && self.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
            return StatusContext::TerminalSearch;
        }
        if self.has_terminal_focus() {
            return StatusContext::Terminal;
        }
        StatusContext::Host
    }

    fn has_terminal_focus(&self) -> bool {
        !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len()
    }

    fn context_split_indicator(&self) -> Span<'static> {
        Span::styled(" || ", Style::default().fg(Color::DarkGray))
    }

    fn selected_host_name(&self) -> Option<String> {
        if let Some(host_idx) = self.selected_host_idx() {
            return self.hosts.get(host_idx).map(|host| host.name.clone());
        }
        if let Some(folder_id) = self.selected_folder_id() {
            return self.folder_by_id(folder_id).map(|folder| format!("Folder: {}", folder.name));
        }
        None
    }

    fn build_manager_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let host_name = self.selected_host_name().unwrap_or_else(|| "none".to_string());
        let mut left = vec![
            Span::styled("Host", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            self.context_split_indicator(),
            Span::styled(host_name, Style::default().fg(Color::White)),
        ];
        if !self.search_query.is_empty() {
            left.push(self.context_split_indicator());
            left.push(Span::styled("filter:", Style::default().fg(Color::DarkGray)));
            left.push(Span::styled(" ", Style::default()));
            left.push(Span::styled(self.search_query.clone(), Style::default().fg(Color::Yellow)));
            left.push(Span::styled(" ", Style::default()));
            left.push(Span::styled("(", Style::default().fg(Color::DarkGray)));
            left.push(Span::styled("^C", Style::default().fg(Color::Red)));
            left.push(Span::styled(" clear)", Style::default().fg(Color::DarkGray)));
        }

        let mut right = vec![
            Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
            Span::styled(":move | ", Style::default().fg(Color::DarkGray)),
            Span::styled("PgUp/Dn", Style::default().fg(Color::Cyan)),
            Span::styled(":page | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Home/End", Style::default().fg(Color::Cyan)),
            Span::styled(":edge | ", Style::default().fg(Color::DarkGray)),
            Span::styled("^F", Style::default().fg(Color::Yellow)),
            Span::styled(":find | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::styled(":open | ", Style::default().fg(Color::DarkGray)),
            Span::styled("c", Style::default().fg(Color::Cyan)),
            Span::styled(":collapse | ", Style::default().fg(Color::DarkGray)),
            Span::styled("i", Style::default().fg(Color::Cyan)),
            Span::styled(":info | ", Style::default().fg(Color::DarkGray)),
            Span::styled("^←/^→", Style::default().fg(Color::Cyan)),
            Span::styled(":resize | ", Style::default().fg(Color::DarkGray)),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::styled(":quick | ", Style::default().fg(Color::DarkGray)),
        ];

        if !self.tabs.is_empty() {
            right.push(Span::styled("S-Tab", Style::default().fg(Color::Cyan)));
            right.push(Span::styled(":tabs | ", Style::default().fg(Color::DarkGray)));
        }

        right.push(Span::styled("^Q", Style::default().fg(Color::Red)));
        right.push(Span::styled(":quit", Style::default().fg(Color::DarkGray)));
        (left, right)
    }

    fn build_terminal_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return (
                vec![
                    Span::styled("Terminal", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                    Span::styled("No active terminal", Style::default().fg(Color::DarkGray)),
                ],
                Vec::new(),
            );
        }

        let tab = &self.tabs[self.selected_tab];
        let is_exited = tab
            .session
            .as_ref()
            .and_then(|session| session.exited.lock().ok().map(|exited| *exited))
            .unwrap_or(true);

        let status_icon_color = if is_exited { Color::Red } else { Color::Green };
        let scroll_info = if tab.scroll_offset > 0 {
            format!(" +{}", tab.scroll_offset)
        } else {
            String::new()
        };

        let mut left = vec![
            Span::styled("Terminal", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            self.context_split_indicator(),
            Span::styled("●", Style::default().fg(status_icon_color).add_modifier(Modifier::BOLD)),
            Span::styled(" ", Style::default()),
            Span::styled(tab.host.name.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ];

        if !scroll_info.is_empty() {
            left.push(Span::styled(" sb:", Style::default().fg(Color::DarkGray)));
            left.push(Span::styled(scroll_info, Style::default().fg(Color::Yellow)));
        }

        let mut right = Vec::new();

        if is_exited {
            right.push(Span::styled("Enter", Style::default().fg(Color::Green)));
            right.push(Span::styled(":reconnect | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("S-Tab", Style::default().fg(Color::Cyan)));
            right.push(Span::styled(":host | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("A-←/→", Style::default().fg(Color::Cyan)));
            right.push(Span::styled(":tab | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("^B", Style::default().fg(Color::Cyan)));
            right.push(Span::styled(":panel | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("^F", Style::default().fg(Color::Cyan)));
            right.push(Span::styled(":find | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("A-c", Style::default().fg(Color::Yellow)));
            right.push(Span::styled(":copy | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("S-PgUp/Dn", Style::default().fg(Color::Yellow)));
            right.push(Span::styled(":scroll | ", Style::default().fg(Color::DarkGray)));
            right.push(Span::styled("^W", Style::default().fg(Color::Red)));
            right.push(Span::styled(":close", Style::default().fg(Color::DarkGray)));
            return (left, right);
        }

        right.extend([
            Span::styled("S-Tab", Style::default().fg(Color::Cyan)),
            Span::styled(":host | ", Style::default().fg(Color::DarkGray)),
            Span::styled("A-←/→", Style::default().fg(Color::Cyan)),
            Span::styled(":tab | ", Style::default().fg(Color::DarkGray)),
            Span::styled("^B", Style::default().fg(Color::Cyan)),
            Span::styled(":panel | ", Style::default().fg(Color::DarkGray)),
            Span::styled("^F", Style::default().fg(Color::Cyan)),
            Span::styled(":find | ", Style::default().fg(Color::DarkGray)),
            Span::styled("A-c", Style::default().fg(Color::Yellow)),
            Span::styled(":copy | ", Style::default().fg(Color::DarkGray)),
            Span::styled("S-PgUp/Dn", Style::default().fg(Color::Yellow)),
            Span::styled(":scroll | ", Style::default().fg(Color::DarkGray)),
            Span::styled("^W", Style::default().fg(Color::Red)),
            Span::styled(":close", Style::default().fg(Color::DarkGray)),
        ]);

        (left, right)
    }

    fn build_search_mode_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let left = vec![
            Span::styled("Host Search", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            self.context_split_indicator(),
            Span::styled(self.search_query.clone(), Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White)),
        ];
        let right = vec![
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red)),
            Span::styled("/", Style::default().fg(Color::DarkGray)),
            Span::styled("^C", Style::default().fg(Color::Red)),
            Span::styled(":clear", Style::default().fg(Color::DarkGray)),
        ];
        (left, right)
    }

    fn build_terminal_search_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let (query, matches_len, current_idx) = if let Some(search) = self.current_tab_search() {
            (search.query.clone(), search.matches.len(), search.current)
        } else {
            (String::new(), 0, 0)
        };

        let match_info = if matches_len > 0 {
            format!("{}/{}", current_idx + 1, matches_len)
        } else {
            "0/0".to_string()
        };

        let left = vec![
            Span::styled("Terminal Search", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            self.context_split_indicator(),
            Span::styled(query, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::White)),
            Span::styled(" ", Style::default()),
            Span::styled(format!("({})", match_info), Style::default().fg(Color::Yellow)),
        ];
        let right = vec![
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red)),
            Span::styled(":clear | ", Style::default().fg(Color::DarkGray)),
            Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
            Span::styled(":next/prev", Style::default().fg(Color::DarkGray)),
        ];
        (left, right)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionManager;
    use ratatui::text::Span;

    #[test]
    fn calculates_span_width_using_unicode_display_width() {
        let app = SessionManager::new_for_tests();
        let spans = vec![Span::raw("a界"), Span::raw("x")];
        assert_eq!(app.spans_display_width(&spans), 4);
    }
}
