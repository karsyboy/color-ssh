//! #_pass prompt modal rendering.

use crate::tui::SessionManager;
use crate::tui::ui::theme;
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

impl SessionManager {
    pub(crate) fn render_pass_prompt_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(prompt) = &self.pass_prompt else {
            return;
        };

        let width = full_area.width.clamp(44, 72);
        let height = 7;
        let area = Self::centered_rect(width, height, full_area);

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Unlock Pass Key ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let label_style = Style::default().fg(theme::ansi_bright_black());
        let value_style = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let cursor_style = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let error_style = Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let hint_style = Style::default().fg(theme::ansi_bright_black());

        let masked = prompt.masked_passphrase();
        let cursor = prompt.cursor.min(masked.chars().count());
        let mut pass_spans = Vec::new();
        if masked.is_empty() {
            pass_spans.push(Span::styled(" ".to_string(), cursor_style));
        } else if cursor < masked.chars().count() {
            let start = char_to_byte_index(&masked, cursor);
            let end = char_to_byte_index(&masked, cursor + 1);
            pass_spans.push(Span::styled(masked[..start].to_string(), value_style));
            pass_spans.push(Span::styled(masked[start..end].to_string(), cursor_style));
            pass_spans.push(Span::styled(masked[end..].to_string(), value_style));
        } else {
            pass_spans.push(Span::styled(masked, value_style));
            pass_spans.push(Span::styled(" ".to_string(), cursor_style));
        }

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Key: ", label_style),
                Span::styled(prompt.pass_key.clone(), value_style),
                Span::styled("  ", label_style),
                Span::styled("Attempts: ", label_style),
                Span::styled(format!("{}", prompt.remaining_attempts()), value_style),
            ]),
            {
                let mut spans = vec![Span::styled("Passphrase: ", label_style)];
                spans.extend(pass_spans);
                Line::from(spans)
            },
        ];

        if let Some(error) = &prompt.error {
            lines.push(Line::from(vec![Span::styled(error.clone(), error_style)]));
        } else {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![Span::styled(
            "[Enter] Unlock  |  [Esc] Continue without auto-login",
            hint_style,
        )]));

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
