//! Host editor rendering.

use super::{HOST_DELETE_CONFIRM_ACTION_SEPARATOR, HOST_DELETE_CONFIRM_CANCEL_LABEL, HOST_DELETE_CONFIRM_DELETE_LABEL};
use crate::tui::features::host_editor::scroll::{
    body_content_width, body_items as editor_body_items, body_scroll_offset as editor_body_scroll_offset, body_viewport_height as editor_body_viewport_height,
    footer_visible_lines as editor_footer_visible_lines, scrollbar_geometry as editor_scrollbar_geometry,
};
use crate::tui::text_edit::{build_edit_value_spans, byte_index_for_char, char_len};
use crate::tui::ui::theme;
use crate::tui::{AppState, HostEditorField, HostEditorMode, HostEditorSection, HostEditorState, HostEditorVisibleItem};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

const SAVE_LABEL: &str = "[ Enter ] Save Entry";
const DELETE_LABEL: &str = "[ Ctrl+D ] Delete Entry";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const ACTION_SEPARATOR: &str = " | ";

fn section_indicator(form: &HostEditorState, section: HostEditorSection) -> &'static str {
    if form.section_collapsed(section) { "▸" } else { "▾" }
}

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
            .title(menu.title());
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
        let selected_section = Style::default().fg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let normal_section = Style::default().fg(theme::ansi_bright_cyan());
        let cursor_value = Style::default().fg(theme::ansi_black()).bg(theme::ansi_cyan()).add_modifier(Modifier::BOLD);
        let placeholder_value = Style::default().fg(theme::ansi_bright_black());
        let save_style = if form.is_selected_field(HostEditorField::Save) {
            Style::default().fg(theme::ansi_green()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let delete_style = if form.is_selected_field(HostEditorField::Delete) {
            Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let cancel_style = if form.is_selected_field(HostEditorField::Cancel) {
            Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ansi_bright_black())
        };
        let separator_style = Style::default().fg(theme::ansi_bright_black());
        let body_items = editor_body_items(form);
        let total_body_lines = body_items.len().saturating_add(2);
        let body_viewport_height = editor_body_viewport_height(inner.height);
        let scroll_offset = editor_body_scroll_offset(form, total_body_lines, body_viewport_height);
        let scrollbar = editor_scrollbar_geometry(inner, total_body_lines, body_viewport_height, scroll_offset);
        let body_width = body_content_width(inner.width, scrollbar);

        let mut body_lines = vec![Line::from(vec![
            Span::styled("File: ", normal_label),
            Span::styled(form.source_file.display().to_string(), normal_value),
        ])];
        body_lines.push(Line::from(""));

        for item in body_items {
            match item {
                HostEditorVisibleItem::SectionHeader(section) => {
                    let section_style = if form.is_selected_section(section) {
                        selected_section
                    } else {
                        normal_section
                    };
                    let indicator = section_indicator(form, section);
                    body_lines.push(Line::from(vec![Span::styled(format!("{indicator} {}", section.label()), section_style)]));
                }
                HostEditorVisibleItem::Field(field) => match field {
                    HostEditorField::FolderPath => {
                        let label_style = if form.is_selected_field(field) { selected_label } else { normal_label };
                        let value_style = if form.is_selected_field(field) { selected_value } else { normal_value };
                        let hint_style = Style::default().fg(theme::ansi_bright_black());
                        let folder_display = form
                            .text_field(HostEditorField::FolderPath)
                            .map(|input| {
                                if input.value.trim().is_empty() {
                                    "/".to_string()
                                } else {
                                    input.value.clone()
                                }
                            })
                            .unwrap_or_else(|| "/".to_string());
                        body_lines.push(Line::from(vec![
                            Span::styled(format!("{}: ", field.label()), label_style),
                            Span::styled(folder_display, value_style),
                            Span::styled("  (Enter to pick)", hint_style),
                        ]));
                    }
                    HostEditorField::IdentitiesOnly => {
                        let label_style = if form.is_selected_field(field) { selected_label } else { normal_label };
                        let value_style = if form.is_selected_field(field) { selected_value } else { normal_value };
                        body_lines.push(Line::from(vec![
                            Span::styled(format!("{}: ", field.label()), label_style),
                            Span::styled(form.identities_only_display(), value_style),
                        ]));
                    }
                    _ => {
                        let label_style = if form.is_selected_field(field) { selected_label } else { normal_label };
                        let placeholder = form.field_example(field);
                        let value_column_width = body_width.saturating_sub(field.label().chars().count() as u16).saturating_sub(2);

                        let value_spans = if let Some(text) = form.text_field(field) {
                            let mut visible_text = text.value.as_str();
                            let mut cursor = form.cursor_for_field(field).unwrap_or(text.cursor);
                            let mut selection = form.selection_for_field(field);

                            if form.is_selected_field(field) {
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

                            if form.is_selected_field(field) {
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
                        body_lines.push(Line::from(spans));
                    }
                },
            }
        }

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
        let hint_line = Line::from(vec![Span::styled(form.hint_text(), separator_style)]);

        let mut action_spans = vec![Span::styled(SAVE_LABEL, save_style)];
        if form.mode == HostEditorMode::Edit {
            action_spans.push(Span::styled(ACTION_SEPARATOR, separator_style));
            action_spans.push(Span::styled(DELETE_LABEL, delete_style));
        }
        action_spans.push(Span::styled(ACTION_SEPARATOR, separator_style));
        action_spans.push(Span::styled(CANCEL_LABEL, cancel_style));
        let action_line = Line::from(action_spans);

        let footer_lines = [message_line, hint_line, action_line];
        let footer_visible = editor_footer_visible_lines(inner.height);
        let footer_start = footer_lines.len().saturating_sub(footer_visible);
        let footer_slice = footer_lines[footer_start..].to_vec();

        if body_viewport_height > 0 && body_width > 0 {
            let body_visible = body_lines.iter().skip(scroll_offset).take(body_viewport_height).cloned().collect::<Vec<_>>();
            let body_area = Rect::new(inner.x, inner.y, body_width, body_viewport_height as u16);
            frame.render_widget(Paragraph::new(body_visible), body_area);
        }

        if footer_visible > 0 {
            let footer_area = Rect::new(inner.x, inner.y.saturating_add(body_viewport_height as u16), inner.width, footer_visible as u16);
            frame.render_widget(Paragraph::new(footer_slice), footer_area);
        }

        if let Some(scrollbar) = scrollbar {
            for row_y in scrollbar.area.y..scrollbar.area.y.saturating_add(scrollbar.area.height) {
                let cell = &mut frame.buffer_mut()[(scrollbar.area.x, row_y)];
                if row_y >= scrollbar.thumb_top && row_y < scrollbar.thumb_top.saturating_add(scrollbar.thumb_height) {
                    cell.set_symbol("█");
                    cell.set_style(Style::default().fg(theme::ansi_cyan()));
                } else {
                    cell.set_symbol("│");
                    cell.set_style(Style::default().fg(theme::ansi_bright_black()));
                }
            }
        }
    }

    pub(crate) fn render_host_delete_confirm_modal(&self, frame: &mut Frame, _full_area: Rect) {
        let Some(confirm) = self.host_delete_confirm.as_ref() else {
            return;
        };
        let Some((area, inner)) = self.host_delete_confirm_modal_layout() else {
            return;
        };

        let trimmed_name = confirm.host_name.trim();
        let compact_name = if trimmed_name.chars().count() > 36 {
            format!("{}...", trimmed_name.chars().take(35).collect::<String>())
        } else {
            trimmed_name.to_string()
        };

        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ansi_red()))
            .title(" Delete Entry ");
        frame.render_widget(block, area);

        let lines = vec![
            Line::from(vec![
                Span::styled("Delete ", Style::default().fg(theme::ansi_bright_white())),
                Span::styled(compact_name, Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD)),
                Span::styled("?", Style::default().fg(theme::ansi_bright_white())),
            ]),
            Line::from(vec![Span::styled("1 entry will be removed.", Style::default().fg(theme::ansi_bright_white()))]),
            Line::from(vec![
                Span::styled(
                    HOST_DELETE_CONFIRM_DELETE_LABEL,
                    Style::default().fg(theme::ansi_red()).add_modifier(Modifier::BOLD),
                ),
                Span::styled(HOST_DELETE_CONFIRM_ACTION_SEPARATOR, Style::default().fg(theme::ansi_bright_black())),
                Span::styled(
                    HOST_DELETE_CONFIRM_CANCEL_LABEL,
                    Style::default().fg(theme::ansi_yellow()).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }
}
