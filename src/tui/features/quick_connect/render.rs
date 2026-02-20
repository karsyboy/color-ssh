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

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    let len = text.chars().count();
    let clamped = char_index.min(len);
    if clamped == len {
        text.len()
    } else {
        text.char_indices().nth(clamped).map_or(text.len(), |(byte_index, _)| byte_index)
    }
}

fn push_if_non_empty(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if !text.is_empty() {
        spans.push(Span::styled(text.to_string(), style));
    }
}

fn build_edit_value_spans(
    text: &str,
    cursor: usize,
    selection: Option<(usize, usize)>,
    value_style: Style,
    cursor_style: Style,
    selection_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let len = text.chars().count();
    let cursor = cursor.min(len);

    if let Some((start_raw, end_raw)) = selection {
        let start = start_raw.min(len);
        let end = end_raw.min(len);
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        if start < end {
            let start_byte = char_to_byte_index(text, start);
            let end_byte = char_to_byte_index(text, end);

            push_if_non_empty(&mut spans, &text[..start_byte], value_style);
            push_if_non_empty(&mut spans, &text[start_byte..end_byte], selection_style);
            push_if_non_empty(&mut spans, &text[end_byte..], value_style);
            return spans;
        }
    }

    if len == 0 {
        spans.push(Span::styled(" ".to_string(), cursor_style));
        return spans;
    }

    if cursor < len {
        let cursor_start = char_to_byte_index(text, cursor);
        let cursor_end = char_to_byte_index(text, cursor + 1);

        push_if_non_empty(&mut spans, &text[..cursor_start], value_style);
        push_if_non_empty(&mut spans, &text[cursor_start..cursor_end], cursor_style);
        push_if_non_empty(&mut spans, &text[cursor_end..], value_style);
    } else {
        spans.push(Span::styled(text.to_string(), value_style));
        spans.push(Span::styled(" ".to_string(), cursor_style));
    }

    spans
}

impl SessionManager {
    // Modal rendering.
    pub(crate) fn render_quick_connect_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(form) = &self.quick_connect else {
            return;
        };

        let width = full_area.width.clamp(44, 74);
        let height = 9;
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
        let cursor_value = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let host_error_active = form.host_required && form.host.trim().is_empty();
        let host_error_label = Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let host_error_value = Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let host_error_cursor = Style::default().fg(theme::ansi_black()).bg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let selected_region = Style::default()
            .fg(theme::selection_fg())
            .bg(theme::selection_bg())
            .add_modifier(Modifier::BOLD);

        let field_style = |field, selected: QuickConnectField| {
            if field == selected { selected_label } else { normal_label }
        };
        let value_style = |field, selected: QuickConnectField| {
            if field == selected { selected_value } else { normal_value }
        };

        let host_label_style = if host_error_active {
            host_error_label
        } else {
            field_style(QuickConnectField::Host, form.selected)
        };

        let host_base_value = if host_error_active {
            host_error_value
        } else {
            value_style(QuickConnectField::Host, form.selected)
        };

        let host_cursor_style = if host_error_active { host_error_cursor } else { cursor_value };

        let user_value_spans: Vec<Span<'static>> = if form.selected == QuickConnectField::User {
            build_edit_value_spans(
                &form.user,
                form.cursor_for_field(QuickConnectField::User).unwrap_or(0),
                form.selection_for_field(QuickConnectField::User),
                selected_value,
                cursor_value,
                selected_region,
            )
        } else if form.user.is_empty() {
            vec![Span::styled("(optional)".to_string(), normal_value)]
        } else {
            vec![Span::styled(form.user.clone(), normal_value)]
        };
        let host_value_spans: Vec<Span<'static>> = if form.selected == QuickConnectField::Host {
            build_edit_value_spans(
                &form.host,
                form.cursor_for_field(QuickConnectField::Host).unwrap_or(0),
                form.selection_for_field(QuickConnectField::Host),
                host_base_value,
                host_cursor_style,
                selected_region,
            )
        } else if form.host.is_empty() {
            vec![Span::styled(String::new(), host_base_value)]
        } else {
            vec![Span::styled(form.host.clone(), host_base_value)]
        };
        let profile_text = form.selected_profile_label().to_string();
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
        let cancel_style = if form.selected == QuickConnectField::Cancel {
            Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };

        let mut user_line_spans = vec![Span::styled("User: ", field_style(QuickConnectField::User, form.selected))];
        user_line_spans.extend(user_value_spans);

        let mut host_line_spans = vec![Span::styled("Host: ", host_label_style)];
        host_line_spans.extend(host_value_spans);

        let lines = vec![
            Line::from(user_line_spans),
            Line::from(host_line_spans),
            Line::from(vec![
                Span::styled("Profile: ", field_style(QuickConnectField::Profile, form.selected)),
                Span::styled(profile_text, value_style(QuickConnectField::Profile, form.selected)),
            ]),
            Line::from(profile_list_spans),
            Line::from(vec![
                Span::styled("SSH Logging: ", field_style(QuickConnectField::Logging, form.selected)),
                Span::styled(format!("{}", logging_mark), value_style(QuickConnectField::Logging, form.selected)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("[ Enter ] Connect", connect_style),
                Span::styled(" | ", Style::default().fg(theme::ansi_bright_black())),
                Span::styled("[ Esc ] Cancel", cancel_style),
            ]),
        ];

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
