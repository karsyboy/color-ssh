//! Folder picker and folder-management modal rendering.

use crate::tui::AppState;
use crate::tui::text_edit::build_edit_value_spans;
use crate::tui::ui::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

const SELECT_ACTION_LABEL: &str = "[ Enter ] Select";
const SAVE_ACTION_LABEL: &str = "[ Enter ] Save";
const CANCEL_ACTION_LABEL: &str = "[ Esc ] Cancel";
const ACTION_SEPARATOR: &str = " | ";

impl AppState {
    pub(crate) fn render_folder_picker_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(picker) = self.folder_picker.as_ref() else {
            return;
        };
        let Some((area, inner)) = self.folder_picker_modal_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(picker.title());
        frame.render_widget(block, area);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("File: ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled(picker.source_file.display().to_string(), Style::default().fg(theme::ansi_bright_white())),
        ]));

        let list_height = inner.height.saturating_sub(2) as usize;
        if list_height > 0 {
            let scroll_offset = Self::folder_picker_scroll_offset(picker.selected, picker.rows.len(), list_height);
            for idx in 0..list_height {
                let row_idx = scroll_offset.saturating_add(idx);
                if let Some(row) = picker.rows.get(row_idx) {
                    let is_selected = row_idx == picker.selected;
                    let row_style = if is_selected {
                        Style::default()
                            .fg(theme::ansi_yellow())
                            .bg(theme::ansi_bright_black())
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme::ansi_bright_white())
                    };

                    let indent = "  ".repeat(row.depth);
                    lines.push(Line::from(vec![Span::styled(format!("{indent}{}", row.label), row_style)]));
                } else {
                    lines.push(Line::from(""));
                }
            }
        }

        lines.push(Line::from(vec![
            Span::styled(SELECT_ACTION_LABEL, Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)),
            Span::styled(ACTION_SEPARATOR, Style::default().fg(theme::ansi_bright_black())),
            Span::styled(CANCEL_ACTION_LABEL, Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
        ]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_folder_rename_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(state) = self.folder_rename.as_ref() else {
            return;
        };
        let Some((area, inner)) = self.folder_rename_modal_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Rename Folder ");
        frame.render_widget(block, area);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Parent: ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled(state.parent_display_path(), Style::default().fg(theme::ansi_bright_white())),
            Span::styled("  (Tab/Ctrl+P to change)", Style::default().fg(theme::ansi_bright_black())),
        ]));

        let cursor_style = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let selected_region = Style::default()
            .fg(theme::selection_fg())
            .bg(theme::selection_bg())
            .add_modifier(Modifier::BOLD);
        let mut name_spans = vec![Span::styled("Name: ", Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD))];
        name_spans.extend(build_edit_value_spans(
            &state.name,
            state.cursor,
            state.selection,
            Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD),
            cursor_style,
            selected_region,
        ));
        lines.push(Line::from(name_spans));

        let message_line = if let Some(error) = state.error.as_ref() {
            Line::from(vec![Span::styled(
                error.clone(),
                Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD),
            )])
        } else {
            Line::from(vec![Span::styled(
                "Use a plain folder name without '/'.",
                Style::default().fg(theme::ansi_bright_black()),
            )])
        };
        lines.push(message_line);

        lines.push(Line::from(vec![
            Span::styled(SAVE_ACTION_LABEL, Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)),
            Span::styled(ACTION_SEPARATOR, Style::default().fg(theme::ansi_bright_black())),
            Span::styled(CANCEL_ACTION_LABEL, Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
        ]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_folder_create_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(state) = self.folder_create.as_ref() else {
            return;
        };
        let Some((area, inner)) = self.folder_create_modal_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" New Folder ");
        frame.render_widget(block, area);

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("Parent: ", Style::default().fg(theme::ansi_bright_black())),
            Span::styled(state.parent_display_path(), Style::default().fg(theme::ansi_bright_white())),
            Span::styled("  (Tab/Ctrl+P to change)", Style::default().fg(theme::ansi_bright_black())),
        ]));

        let cursor_style = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let selected_region = Style::default()
            .fg(theme::selection_fg())
            .bg(theme::selection_bg())
            .add_modifier(Modifier::BOLD);
        let mut name_spans = vec![Span::styled("Name: ", Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD))];
        name_spans.extend(build_edit_value_spans(
            &state.name,
            state.cursor,
            state.selection,
            Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD),
            cursor_style,
            selected_region,
        ));
        lines.push(Line::from(name_spans));

        let message_line = if let Some(error) = state.error.as_ref() {
            Line::from(vec![Span::styled(
                error.clone(),
                Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD),
            )])
        } else {
            Line::from(vec![Span::styled(
                "Use a plain folder name without '/'.",
                Style::default().fg(theme::ansi_bright_black()),
            )])
        };
        lines.push(message_line);

        lines.push(Line::from(vec![
            Span::styled(SAVE_ACTION_LABEL, Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)),
            Span::styled(ACTION_SEPARATOR, Style::default().fg(theme::ansi_bright_black())),
            Span::styled(CANCEL_ACTION_LABEL, Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)),
        ]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_folder_delete_confirm_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(confirm) = self.folder_delete_confirm.as_ref() else {
            return;
        };
        let Some((area, inner)) = self.folder_delete_confirm_modal_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_red()))
            .title(" Delete Folder ");
        frame.render_widget(block, area);

        let mode_line = match confirm.removed_entry_count {
            0 => "No entries will be removed.".to_string(),
            1 => "1 entry will be removed.".to_string(),
            count => format!("{count} entries will be removed."),
        };
        let delete_target = format!("{} ({})", confirm.folder_name, confirm.folder_display_path());

        let lines = vec![
            Line::from(vec![
                Span::styled("Delete ", Style::default().fg(theme::ansi_bright_white())),
                Span::styled(delete_target, Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)),
                Span::styled("?", Style::default().fg(theme::ansi_bright_white())),
            ]),
            Line::from(vec![Span::styled(mode_line, Style::default().fg(theme::ansi_bright_white()))]),
            Line::from(vec![Span::styled(
                "Enter/y delete · Esc/n cancel",
                Style::default().fg(theme::ansi_bright_black()),
            )]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
