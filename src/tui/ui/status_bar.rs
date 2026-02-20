//! Global status bar rendering.

use crate::tui::SessionManager;
use crate::tui::ui::theme::{self, display_width};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
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
    // Entry point.
    pub(crate) fn render_global_status_bar(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (left_spans, right_spans) = self.build_status_line_sections();
        let base_style = Style::default().fg(theme::ansi_white());

        if right_spans.is_empty() {
            let status = Paragraph::new(Line::from(left_spans)).style(base_style);
            frame.render_widget(status, area);
            return;
        }

        let right_width = Self::spans_display_width(&right_spans).min(area.width as usize) as u16;
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(right_width)])
            .split(area);

        let left = Paragraph::new(Line::from(left_spans)).style(base_style);
        let right = Paragraph::new(Line::from(right_spans)).style(base_style).alignment(Alignment::Right);
        frame.render_widget(left, chunks[0]);
        frame.render_widget(right, chunks[1]);
    }

    // Context dispatch.
    fn build_status_line_sections(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        match self.resolve_status_context() {
            StatusContext::HostSearch => self.build_search_mode_status_spans(),
            StatusContext::TerminalSearch => self.build_terminal_search_status_spans(),
            StatusContext::Host => self.build_manager_status_spans(),
            StatusContext::Terminal => self.build_terminal_status_spans(),
        }
    }

    // Shared helpers.
    fn spans_display_width(spans: &[Span<'static>]) -> usize {
        spans.iter().map(|span| display_width(span.content.as_ref())).sum()
    }

    fn resolve_status_context(&self) -> StatusContext {
        if self.search_mode {
            return StatusContext::HostSearch;
        }
        if self.has_terminal_focus() && self.current_tab_search().is_some_and(|search_state| search_state.active) {
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

    // Context snippets.
    fn context_split_indicator() -> Span<'static> {
        Span::styled(" || ", Style::default().fg(theme::ansi_bright_black()))
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

    // Host browser context.
    fn build_manager_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let host_name = self.selected_host_name().unwrap_or_else(|| "none".to_string());
        let mut left = vec![
            Span::styled("Host", Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)),
            Self::context_split_indicator(),
            Span::styled(host_name, Style::default().fg(theme::ansi_bright_white())),
        ];
        if !self.search_query.is_empty() {
            left.push(Self::context_split_indicator());
            left.push(Span::styled("filter:", Style::default().fg(theme::ansi_bright_black())));
            left.push(Span::styled(" ", Style::default()));
            left.push(Span::styled(self.search_query.clone(), Style::default().fg(theme::ansi_yellow())));
            left.push(Span::styled(" ", Style::default()));
            left.push(Span::styled("(", Style::default().fg(theme::ansi_bright_black())));
            left.push(Span::styled("^C", Style::default().fg(theme::ansi_red())));
            left.push(Span::styled(" clear)", Style::default().fg(theme::ansi_bright_black())));
        }

        let mut right = vec![
            Span::styled("↑/↓", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":move | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("PgUp/Dn", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":page | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("Home/End", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":edge | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^F", Style::default().fg(theme::ansi_yellow())),
            Span::styled(":find | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("Enter", Style::default().fg(theme::ansi_green())),
            Span::styled(":open | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("c", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":collapse | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("i", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":info | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^←/^→", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":resize | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("q", Style::default().fg(theme::ansi_yellow())),
            Span::styled(":quick | ", Style::default().fg(theme::ansi_bright_black())),
        ];

        if !self.tabs.is_empty() {
            right.push(Span::styled("S-Tab", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":tabs | ", Style::default().fg(theme::ansi_bright_black())));
        }

        right.push(Span::styled("^Q", Style::default().fg(theme::ansi_red())));
        right.push(Span::styled(":quit", Style::default().fg(theme::ansi_bright_black())));
        (left, right)
    }

    // Terminal context.
    fn build_terminal_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return (
                vec![
                    Span::styled("Terminal", Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
                    Span::styled(" | ", Style::default().fg(theme::ansi_bright_black())),
                    Span::styled("No active terminal", Style::default().fg(theme::ansi_bright_black())),
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

        let status_icon_color = if is_exited { theme::ansi_red() } else { theme::ansi_green() };
        let scroll_info = if tab.scroll_offset > 0 {
            format!(" +{}", tab.scroll_offset)
        } else {
            String::new()
        };

        let mut left = vec![
            Span::styled("Terminal", Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
            Self::context_split_indicator(),
            Span::styled("●", Style::default().fg(status_icon_color).add_modifier(Modifier::BOLD)),
            Span::styled(" ", Style::default()),
            Span::styled(tab.host.name.clone(), Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)),
        ];

        if !scroll_info.is_empty() {
            left.push(Span::styled(" sb:", Style::default().fg(theme::ansi_bright_black())));
            left.push(Span::styled(scroll_info, Style::default().fg(theme::ansi_yellow())));
        }

        let mut right = Vec::new();

        if is_exited {
            right.push(Span::styled("Enter", Style::default().fg(theme::ansi_green())));
            right.push(Span::styled(":reconnect | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("S-Tab", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":host | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("A-←/→", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":tab | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("^←/^→", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":move | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("^B", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":panel | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("^F", Style::default().fg(theme::ansi_cyan())));
            right.push(Span::styled(":find | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("A-c", Style::default().fg(theme::ansi_yellow())));
            right.push(Span::styled(":copy | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("S-PgUp/Dn", Style::default().fg(theme::ansi_yellow())));
            right.push(Span::styled(":scroll | ", Style::default().fg(theme::ansi_bright_black())));
            right.push(Span::styled("^W", Style::default().fg(theme::ansi_red())));
            right.push(Span::styled(":close", Style::default().fg(theme::ansi_bright_black())));
            return (left, right);
        }

        right.extend([
            Span::styled("S-Tab", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":host | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("A-←/→", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":tab | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^←/^→", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":move | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^B", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":panel | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^F", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":find | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("A-c", Style::default().fg(theme::ansi_yellow())),
            Span::styled(":copy | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("S-PgUp/Dn", Style::default().fg(theme::ansi_yellow())),
            Span::styled(":scroll | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^W", Style::default().fg(theme::ansi_red())),
            Span::styled(":close", Style::default().fg(theme::ansi_bright_black())),
        ]);

        (left, right)
    }

    // Host search context.
    fn build_search_mode_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let left = vec![
            Span::styled("Host Search", Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)),
            Self::context_split_indicator(),
            Span::styled(self.search_query.clone(), Style::default().fg(theme::ansi_bright_white())),
            Span::styled("_", Style::default().fg(theme::ansi_bright_white())),
        ];
        let right = vec![
            Span::styled("Enter", Style::default().fg(theme::ansi_green())),
            Span::styled(" | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("Esc", Style::default().fg(theme::ansi_red())),
            Span::styled("/", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("^C", Style::default().fg(theme::ansi_red())),
            Span::styled(":clear", Style::default().fg(theme::ansi_bright_black())),
        ];
        (left, right)
    }

    // Terminal search context.
    fn build_terminal_search_status_spans(&self) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
        let (query, matches_len, current_idx) = self
            .current_tab_search()
            .map_or_else(|| (String::new(), 0, 0), |search| (search.query.clone(), search.matches.len(), search.current));

        let match_info = if matches_len > 0 {
            format!("{}/{}", current_idx + 1, matches_len)
        } else {
            "0/0".to_string()
        };

        let left = vec![
            Span::styled("Terminal Search", Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)),
            Self::context_split_indicator(),
            Span::styled(query, Style::default().fg(theme::ansi_bright_white())),
            Span::styled("_", Style::default().fg(theme::ansi_bright_white())),
            Span::styled(" ", Style::default()),
            Span::styled(format!("({match_info})"), Style::default().fg(theme::ansi_yellow())),
        ];
        let right = vec![
            Span::styled("Enter", Style::default().fg(theme::ansi_green())),
            Span::styled(" | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("Esc", Style::default().fg(theme::ansi_red())),
            Span::styled(":clear | ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled("↑/↓", Style::default().fg(theme::ansi_cyan())),
            Span::styled(":next/prev", Style::default().fg(theme::ansi_bright_black())),
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
        let spans = vec![Span::raw("a界"), Span::raw("x")];
        assert_eq!(SessionManager::spans_display_width(&spans), 4);
    }
}
