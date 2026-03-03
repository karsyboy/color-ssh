//! Password vault unlock modal rendering.

use crate::tui::SessionManager;
use crate::tui::ui::theme;
use chrono::{Local, TimeZone};
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

fn format_vault_time_left(seconds: Option<u64>) -> String {
    let Some(total_seconds) = seconds else {
        return "n/a".to_string();
    };

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_vault_timeout_at(epoch_seconds: Option<u64>) -> String {
    let Some(epoch_seconds) = epoch_seconds else {
        return "n/a".to_string();
    };

    Local
        .timestamp_opt(epoch_seconds as i64, 0)
        .single()
        .map(|datetime| datetime.format("%a %m-%d-%Y %I:%M:%S %p").to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

impl SessionManager {
    pub(crate) fn render_vault_unlock_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(prompt) = &self.vault_unlock else {
            return;
        };

        let width = full_area.width.clamp(44, 72);
        let height = 6;
        let area = Self::centered_rect(width, height, full_area);

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Unlock Password Vault ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let label_style = Style::default().fg(theme::ansi_bright_black());
        let value_style = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let cursor_style = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let error_style = Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD);
        let hint_style = Style::default().fg(theme::ansi_bright_black());

        let masked = prompt.masked_master_password();
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
                Span::styled(prompt.action.prompt_target_label(), label_style),
                Span::styled(prompt.action.prompt_target_value(&prompt.entry_name).to_string(), value_style),
                Span::styled("  ", label_style),
                Span::styled("Attempts: ", label_style),
                Span::styled(format!("{}", prompt.remaining_attempts()), value_style),
            ]),
            {
                let mut spans = vec![Span::styled("Master Password: ", label_style)];
                spans.extend(pass_spans);
                Line::from(spans)
            },
        ];

        if let Some(error) = &prompt.error {
            lines.push(Line::from(vec![Span::styled(error.clone(), error_style)]));
        } else {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![Span::styled(prompt.action.prompt_hint(), hint_style)]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_vault_status_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(modal) = &self.vault_status_modal else {
            return;
        };

        let width = full_area.width.clamp(52, 80);
        let height = 7;
        let area = Self::centered_rect(width, height, full_area);

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Password Vault Status ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let label_style = Style::default().fg(theme::ansi_bright_black());
        let status_style = if self.vault_status.unlocked {
            Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)
        };
        let value_style = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let message_style = if modal.message_is_error {
            Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)
        };
        let hint_style = Style::default().fg(theme::ansi_bright_black());
        let hint_text = if self.vault_status.unlocked {
            "[l] Lock  |  [Enter / Esc / v] Close"
        } else {
            "[v] Unlock  |  [Enter / Esc] Close"
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Status: ", label_style),
                Span::styled(if self.vault_status.unlocked { "Unlocked" } else { "Locked" }, status_style),
            ]),
            Line::from(vec![
                Span::styled("Time Left: ", label_style),
                Span::styled(format_vault_time_left(self.vault_status.unlock_expires_in_seconds), value_style),
            ]),
            Line::from(vec![
                Span::styled("Absolute Session Timeout: ", label_style),
                Span::styled(format_vault_timeout_at(self.vault_status.absolute_timeout_at_epoch_seconds), value_style),
            ]),
            modal
                .message
                .as_ref()
                .map_or_else(|| Line::from(""), |message| Line::from(vec![Span::styled(message.clone(), message_style)])),
            Line::from(vec![Span::styled(hint_text, hint_style)]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
#[path = "../../../test/tui/features/pass_prompt/render.rs"]
mod tests;
