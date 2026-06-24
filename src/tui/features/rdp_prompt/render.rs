//! Launch-time RDP credential modal rendering.

use crate::tui::text_edit::{build_edit_value_spans, byte_index_for_char};
use crate::tui::ui::theme;
use crate::tui::{AppState, RdpCredentialsField};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

impl AppState {
    pub(crate) fn render_rdp_credentials_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(prompt) = &self.rdp_credentials else {
            return;
        };

        let Some((area, inner)) = self.rdp_credentials_modal_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" RDP Credentials ");
        frame.render_widget(block, area);

        let selected_label = Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD);
        let normal_label = Style::default().fg(theme::ansi_bright_black());
        let selected_value = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let normal_value = Style::default().fg(theme::ansi_bright_white());
        let cursor_value = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let selected_region = Style::default()
            .fg(theme::selection_fg())
            .bg(theme::selection_bg())
            .add_modifier(Modifier::BOLD);
        let error_style = Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let notice_style = Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD);
        let hint_style = Style::default().fg(theme::ansi_bright_black());

        let field_style = |field, selected: RdpCredentialsField| {
            if field == selected { selected_label } else { normal_label }
        };

        let user_spans = if prompt.selected == RdpCredentialsField::User {
            build_edit_value_spans(
                prompt.text_for_field(RdpCredentialsField::User).unwrap_or_default(),
                prompt.cursor_for_field(RdpCredentialsField::User),
                prompt.selection_for_field(RdpCredentialsField::User),
                selected_value,
                cursor_value,
                selected_region,
            )
        } else if prompt.user.trim().is_empty() {
            vec![Span::styled("(required)", normal_value)]
        } else {
            vec![Span::styled(prompt.user.as_str(), normal_value)]
        };

        let domain_spans = if prompt.selected == RdpCredentialsField::Domain {
            build_edit_value_spans(
                prompt.text_for_field(RdpCredentialsField::Domain).unwrap_or_default(),
                prompt.cursor_for_field(RdpCredentialsField::Domain),
                prompt.selection_for_field(RdpCredentialsField::Domain),
                selected_value,
                cursor_value,
                selected_region,
            )
        } else if prompt.domain.trim().is_empty() {
            vec![Span::styled("(optional)", normal_value)]
        } else {
            vec![Span::styled(prompt.domain.as_str(), normal_value)]
        };

        let port_spans = if prompt.selected == RdpCredentialsField::Port {
            build_edit_value_spans(
                prompt.text_for_field(RdpCredentialsField::Port).unwrap_or_default(),
                prompt.cursor_for_field(RdpCredentialsField::Port),
                prompt.selection_for_field(RdpCredentialsField::Port),
                selected_value,
                cursor_value,
                selected_region,
            )
        } else if prompt.port.trim().is_empty() {
            vec![Span::styled("(optional)", normal_value)]
        } else {
            vec![Span::styled(prompt.port.as_str(), normal_value)]
        };

        let password_spans = {
            let masked = prompt.masked_password();
            if prompt.selected == RdpCredentialsField::Password {
                let cursor = prompt.cursor_for_field(RdpCredentialsField::Password).min(masked.chars().count());
                if masked.is_empty() {
                    vec![Span::styled(" ".to_string(), cursor_value)]
                } else if cursor < masked.chars().count() {
                    let start = byte_index_for_char(&masked, cursor);
                    let end = byte_index_for_char(&masked, cursor + 1);
                    vec![
                        Span::styled(masked[..start].to_string(), selected_value),
                        Span::styled(masked[start..end].to_string(), cursor_value),
                        Span::styled(masked[end..].to_string(), selected_value),
                    ]
                } else {
                    vec![Span::styled(masked, selected_value), Span::styled(" ".to_string(), cursor_value)]
                }
            } else if masked.is_empty() {
                vec![Span::styled("(optional)", normal_value)]
            } else {
                vec![Span::styled(masked, normal_value)]
            }
        };

        let message_line = if let Some(error) = &prompt.error {
            Line::from(vec![Span::styled(error.clone(), error_style)])
        } else if let Some(notice) = &prompt.notice {
            Line::from(vec![Span::styled(notice.clone(), notice_style)])
        } else {
            let hint = match &prompt.action {
                crate::tui::RdpCredentialsAction::OpenHostTab { launch_context, .. }
                | crate::tui::RdpCredentialsAction::ReconnectTab { launch_context, .. } => {
                    if launch_context.pass_entry_override.is_some() {
                        "Leave password blank to use the unlocked vault entry."
                    } else {
                        "Leave password blank to continue with the FreeRDP password prompt."
                    }
                }
            };
            Line::from(vec![Span::styled(hint, hint_style)])
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Target: ", normal_label),
                Span::styled(prompt.target_label.as_str(), selected_value),
            ]),
            Line::from({
                let mut spans = vec![Span::styled("User: ", field_style(RdpCredentialsField::User, prompt.selected))];
                spans.extend(user_spans);
                spans
            }),
            Line::from({
                let mut spans = vec![Span::styled("Domain: ", field_style(RdpCredentialsField::Domain, prompt.selected))];
                spans.extend(domain_spans);
                spans
            }),
            Line::from({
                let mut spans = vec![Span::styled("Port: ", field_style(RdpCredentialsField::Port, prompt.selected))];
                spans.extend(port_spans);
                spans
            }),
            Line::from({
                let mut spans = vec![Span::styled("Password: ", field_style(RdpCredentialsField::Password, prompt.selected))];
                spans.extend(password_spans);
                spans
            }),
            message_line,
            Line::from(vec![Span::styled("[Enter] Launch  |  [Esc] Cancel", hint_style)]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
