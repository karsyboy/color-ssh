//! Quick-connect modal rendering.

use crate::tui::ui::theme;
use crate::tui::{QuickConnectField, SessionManager};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

impl SessionManager {
    // Modal rendering.
    pub(crate) fn render_quick_connect_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(form) = &self.quick_connect else {
            return;
        };

        let width = full_area.width.clamp(44, 74);
        let height = if form.error.is_some() { 12 } else { 11 };
        let area = Self::centered_rect(width, height, full_area);

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Quick Connect ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let selected_label = Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD);
        let normal_label = Style::default().fg(theme::ansi_bright_black());
        let selected_value = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let normal_value = Style::default().fg(theme::ansi_bright_white());

        let field_style = |field, selected: QuickConnectField| {
            if field == selected { selected_label } else { normal_label }
        };
        let value_style = |field, selected: QuickConnectField| {
            if field == selected { selected_value } else { normal_value }
        };

        let user_text = if form.selected == QuickConnectField::User {
            format!("{}_", form.user)
        } else {
            form.user.clone()
        };
        let host_text = if form.selected == QuickConnectField::Host {
            format!("{}_", form.host)
        } else {
            form.host.clone()
        };
        let profile_text = form.selected_profile_label().to_string();
        let profile_text = if form.selected == QuickConnectField::Profile {
            format!("{}_", profile_text)
        } else {
            profile_text
        };
        let mut profile_list_spans = vec![Span::styled("Profiles: ", Style::default().fg(theme::ansi_bright_black()))];
        for (idx, profile_name) in form.profile_options.iter().enumerate() {
            if idx > 0 {
                profile_list_spans.push(Span::styled(" | ", Style::default().fg(theme::ansi_bright_black())));
            }
            let style = if idx == form.profile_index {
                Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::ansi_white())
            };
            profile_list_spans.push(Span::styled(profile_name.clone(), style));
        }

        let logging_mark = if form.ssh_logging { "[x]" } else { "[ ]" };
        let connect_style = if form.selected == QuickConnectField::Connect {
            Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("User: ", field_style(QuickConnectField::User, form.selected)),
                Span::styled(
                    if user_text.is_empty() { "(optional)".to_string() } else { user_text },
                    value_style(QuickConnectField::User, form.selected),
                ),
            ]),
            Line::from(vec![
                Span::styled("Host: ", field_style(QuickConnectField::Host, form.selected)),
                Span::styled(host_text, value_style(QuickConnectField::Host, form.selected)),
            ]),
            Line::from(vec![
                Span::styled("Profile: ", field_style(QuickConnectField::Profile, form.selected)),
                Span::styled(profile_text, value_style(QuickConnectField::Profile, form.selected)),
            ]),
            Line::from(profile_list_spans),
            Line::from(vec![
                Span::styled("SSH Logging: ", field_style(QuickConnectField::Logging, form.selected)),
                Span::styled(format!("{} (-l)", logging_mark), value_style(QuickConnectField::Logging, form.selected)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled("[ Enter ] Connect", connect_style)]),
            Line::from(vec![Span::styled(
                "Esc: cancel | Tab/Shift+Tab: field | ←/→: profile",
                Style::default().fg(theme::ansi_bright_black()),
            )]),
        ];

        if let Some(error) = &form.error {
            lines.push(Line::from(vec![Span::styled(error.clone(), Style::default().fg(theme::ansi_red()))]));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    // Shared centered popup helper.
    pub(crate) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
        let popup_width = width.min(area.width);
        let popup_height = height.min(area.height);
        let popup_x = area.x + area.width.saturating_sub(popup_width) / 2;
        let popup_y = area.y + area.height.saturating_sub(popup_height) / 2;
        Rect::new(popup_x, popup_y, popup_width, popup_height)
    }
}
