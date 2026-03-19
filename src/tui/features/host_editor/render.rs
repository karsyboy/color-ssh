//! Host editor rendering.

use crate::tui::text_edit::{build_edit_value_spans, byte_index_for_char, char_len};
use crate::tui::ui::theme;
use crate::tui::{AppState, HostEditorField, HostEditorMode, HostEditorState};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

const SAVE_LABEL: &str = "[ Enter ] Save Entry";
const DELETE_LABEL: &str = "[ d ] Delete Entry";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const ACTION_SEPARATOR: &str = " | ";

impl AppState {
    pub(crate) fn render_host_context_menu(&self, frame: &mut Frame) {
        let Some(menu) = self.host_context_menu.as_ref() else {
            return;
        };

        let Some((area, inner)) = self.host_context_menu_layout() else {
            return;
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(" Host Menu ");
        frame.render_widget(block, area);

        let lines = menu
            .actions
            .iter()
            .enumerate()
            .map(|(idx, action)| {
                let selected = idx == menu.selected;
                let style = if selected {
                    Style::default()
                        .fg(theme::ansi_yellow())
                        .bg(theme::ansi_bright_black())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::ansi_bright_white())
                };

                Line::from(Span::styled(action.label(), style))
            })
            .collect::<Vec<_>>();

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_host_editor_tab(&self, frame: &mut Frame, full_area: Rect, form: &HostEditorState) {
        if full_area.width < 2 || full_area.height < 2 {
            return;
        }

        let area = full_area;
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_cyan()))
            .title(form.title());
        frame.render_widget(block, area);

        let selected_label = Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD);
        let normal_label = Style::default().fg(theme::ansi_bright_black());
        let selected_value = Style::default().fg(theme::ansi_bright_white()).add_modifier(Modifier::BOLD);
        let normal_value = Style::default().fg(theme::ansi_bright_white());
        let cursor_value = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let placeholder_value = Style::default().fg(theme::ansi_bright_black());
        let save_style = if form.selected == HostEditorField::Save {
            Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let delete_style = if form.selected == HostEditorField::Delete {
            Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let cancel_style = if form.selected == HostEditorField::Cancel {
            Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let separator_style = Style::default().fg(theme::ansi_bright_black());

        let mut lines = vec![Line::from(vec![
            Span::styled("File: ", normal_label),
            Span::styled(form.source_file.display().to_string(), normal_value),
        ])];
        lines.push(Line::from(""));

        for field in form.visible_fields() {
            if field.is_action() {
                continue;
            }

            match field {
                HostEditorField::Hidden => {
                    let label_style = if form.selected == field { selected_label } else { normal_label };
                    let value_style = if form.selected == field { selected_value } else { normal_value };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{}: ", field.label()), label_style),
                        Span::styled(form.hidden_display(), value_style),
                    ]));
                }
                HostEditorField::IdentitiesOnly => {
                    let label_style = if form.selected == field { selected_label } else { normal_label };
                    let value_style = if form.selected == field { selected_value } else { normal_value };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{}: ", field.label()), label_style),
                        Span::styled(form.identities_only_display(), value_style),
                    ]));
                }
                _ => {
                    let label_style = if form.selected == field { selected_label } else { normal_label };
                    let placeholder = form.field_example(field);
                    let value_column_width = inner.width.saturating_sub(field.label().chars().count() as u16).saturating_sub(2);

                    let value_spans = if let Some(text) = form.text_field(field) {
                        let mut visible_text = text.value.as_str();
                        let mut cursor = form.cursor_for_field(field).unwrap_or(text.cursor);
                        let mut selection = form.selection_for_field(field);

                        if form.selected == field {
                            let scroll_offset = form.field_horizontal_scroll_offset(field, value_column_width);
                            let visible_limit = value_column_width as usize;
                            let start_byte = byte_index_for_char(text.value.as_str(), scroll_offset);
                            let end_byte = byte_index_for_char(text.value.as_str(), scroll_offset.saturating_add(visible_limit));
                            visible_text = &text.value[start_byte..end_byte];

                            let visible_len = char_len(visible_text);
                            cursor = cursor.saturating_sub(scroll_offset).min(visible_len);
                            selection = selection.map(|(start, end)| {
                                let start = start.saturating_sub(scroll_offset).min(visible_len);
                                let end = end.saturating_sub(scroll_offset).min(visible_len);
                                (start, end)
                            });
                        }

                        if form.selected == field {
                            if visible_text.is_empty() {
                                if let Some(example) = placeholder {
                                    vec![Span::styled(" ", cursor_value), Span::styled(example, placeholder_value)]
                                } else {
                                    build_edit_value_spans(visible_text, cursor, selection, selected_value, cursor_value, cursor_value)
                                }
                            } else {
                                build_edit_value_spans(visible_text, cursor, selection, selected_value, cursor_value, cursor_value)
                            }
                        } else if text.value.is_empty() {
                            if let Some(example) = placeholder {
                                vec![Span::styled(example, placeholder_value)]
                            } else {
                                vec![Span::styled("", normal_value)]
                            }
                        } else {
                            vec![Span::styled(text.value.as_str(), normal_value)]
                        }
                    } else {
                        vec![Span::styled("", normal_value)]
                    };

                    let mut spans = vec![Span::styled(format!("{}: ", field.label()), label_style)];
                    spans.extend(value_spans);
                    lines.push(Line::from(spans));
                }
            }
        }

        lines.push(Line::from(""));

        let message_line = if let Some(error) = &form.error {
            Line::from(vec![Span::styled(
                error.clone(),
                Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD),
            )])
        } else {
            let help_text = if form.mode == HostEditorMode::Create {
                "List/map fields accept YAML-compatible input (for example [\"a\", \"b\"] or {Opt: [\"v\"]})."
            } else {
                "List/map fields accept YAML-compatible input; unknown custom keys are preserved on save."
            };
            Line::from(vec![Span::styled(help_text, Style::default().fg(theme::ansi_bright_black()))])
        };
        lines.push(message_line);

        lines.push(Line::from(vec![Span::styled(form.hint_text(), separator_style)]));

        let mut action_spans = vec![Span::styled(SAVE_LABEL, save_style)];
        if form.mode == HostEditorMode::Edit {
            action_spans.push(Span::styled(ACTION_SEPARATOR, separator_style));
            action_spans.push(Span::styled(DELETE_LABEL, delete_style));
        }
        action_spans.push(Span::styled(ACTION_SEPARATOR, separator_style));
        action_spans.push(Span::styled(CANCEL_LABEL, cancel_style));
        lines.push(Line::from(action_spans));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    pub(crate) fn render_host_delete_confirm_modal(&self, frame: &mut Frame, full_area: Rect) {
        let Some(confirm) = self.host_delete_confirm.as_ref() else {
            return;
        };

        let width = full_area.width.clamp(52, 92);
        let height = 6;
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_red()))
            .title(" Confirm Delete ");
        frame.render_widget(block, area);

        let lines = vec![
            Line::from(vec![
                Span::styled("Delete entry ", Style::default().fg(theme::ansi_bright_white())),
                Span::styled(
                    format!("'{}'", confirm.host_name),
                    Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD),
                ),
                Span::styled("?", Style::default().fg(theme::ansi_bright_white())),
            ]),
            Line::from(vec![Span::styled(
                "[y/Enter] Delete  |  [Esc/n] Cancel",
                Style::default().fg(theme::ansi_bright_black()),
            )]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
