//! Host editor mouse helpers.

use crate::tui::{AppState, HostEditorField, HostEditorState, HostEditorVisibleItem};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const SAVE_LABEL: &str = "[ Enter ] Save Entry";
const DELETE_LABEL: &str = "[ d ] Delete Entry";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const ACTION_SEPARATOR: &str = " | ";
// Rendered lines between the last editable field and the bottom action row:
// blank spacer + message + hint.
const ACTION_ROW_OFFSET_AFTER_FIELDS: usize = 3;

impl AppState {
    pub(crate) fn host_context_menu_layout(&self) -> Option<(Rect, Rect)> {
        let menu = self.host_context_menu.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);

        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let label_width = menu
            .actions
            .iter()
            .map(|action| action.label().chars().count() as u16)
            .max()
            .unwrap_or(1)
            .max(1);
        let menu_width = (label_width + 2).max(10).saturating_add(2).min(full_area.width);
        let menu_height = (menu.actions.len() as u16).max(1).saturating_add(2).min(full_area.height);

        let max_x = full_area.x.saturating_add(full_area.width.saturating_sub(menu_width));
        let max_y = full_area.y.saturating_add(full_area.height.saturating_sub(menu_height));

        let menu_x = menu.column.min(max_x);
        let menu_y = menu.row.min(max_y);

        let area = Rect::new(menu_x, menu_y, menu_width, menu_height);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn handle_host_context_menu_mouse(&mut self, mouse: event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some((area, inner)) = self.host_context_menu_layout() else {
                    return;
                };

                if !Self::host_editor_point_in_rect(area, mouse.column, mouse.row) {
                    self.host_context_menu = None;
                    self.mark_ui_dirty();
                    return;
                }

                if !Self::host_editor_point_in_rect(inner, mouse.column, mouse.row) {
                    return;
                }

                let local_row = mouse.row.saturating_sub(inner.y) as usize;
                if let Some(menu) = self.host_context_menu.as_mut()
                    && local_row < menu.actions.len()
                {
                    menu.selected = local_row;
                    if let Some(action) = menu.selected_action() {
                        self.execute_host_context_menu_action(action);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.host_context_menu = None;
            }
            _ => {}
        }
    }

    pub(crate) fn host_editor_tab_layout(&self, tab_content_area: Rect) -> Option<(Rect, Rect)> {
        self.selected_host_editor()?;

        if tab_content_area.width == 0 || tab_content_area.height == 0 {
            return None;
        }

        let area = tab_content_area;
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        Some((area, inner))
    }

    pub(crate) fn handle_host_editor_mouse(&mut self, mouse: event::MouseEvent, tab_content_area: Rect) {
        let Some((area, inner_area)) = self.host_editor_tab_layout(tab_content_area) else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::host_editor_point_in_rect(area, mouse.column, mouse.row) {
                    if let Some(form) = self.selected_host_editor_mut() {
                        form.finish_mouse_selection();
                    }
                    return;
                }

                if !Self::host_editor_point_in_rect(inner_area, mouse.column, mouse.row) {
                    if let Some(form) = self.selected_host_editor_mut() {
                        form.finish_mouse_selection();
                    }
                    return;
                }

                self.handle_host_editor_left_click(mouse.column, mouse.row, inner_area);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_host_editor_left_drag(mouse.column, inner_area);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.handle_host_editor_left_release(mouse.column, inner_area);
            }
            _ => {}
        }
    }

    fn handle_host_editor_left_click(&mut self, mouse_col: u16, mouse_row: u16, inner_area: Rect) {
        let local_row = mouse_row.saturating_sub(inner_area.y);
        let mut should_submit = false;
        let mut should_close = false;
        let mut should_open_delete_confirm = false;
        let item = self.host_editor_item_at_point(local_row, mouse_col, inner_area);

        if let Some(form) = self.selected_host_editor_mut() {
            form.finish_mouse_selection();

            let Some(item) = item else {
                return;
            };
            form.selected = item;

            match item {
                HostEditorVisibleItem::SectionHeader(section) => {
                    form.toggle_section(section);
                }
                HostEditorVisibleItem::Field(field) => match field {
                    HostEditorField::Save => {
                        should_submit = true;
                    }
                    HostEditorField::Delete => {
                        should_open_delete_confirm = true;
                    }
                    HostEditorField::Cancel => {
                        should_close = true;
                    }
                    HostEditorField::IdentitiesOnly => {
                        form.cycle_identities_only_forward();
                    }
                    HostEditorField::Protocol => {}
                    HostEditorField::FolderPath => {
                        self.open_folder_picker_for_editor_placement();
                    }
                    _ => {
                        if let Some(offset) = Self::host_editor_text_offset(form, inner_area, field, mouse_col) {
                            form.begin_mouse_selection(field, offset);
                        }
                    }
                },
            }
        }

        if should_submit {
            self.submit_host_editor();
        } else if should_open_delete_confirm {
            self.open_host_delete_confirmation();
        } else if should_close {
            self.host_delete_confirm = None;
            self.close_selected_editor_tab();
        }
    }

    fn handle_host_editor_left_drag(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(form) = self.selected_host_editor_mut() else {
            return;
        };
        let Some(field) = form.mouse_drag_field() else {
            return;
        };

        if let Some(offset) = Self::host_editor_text_offset(form, inner_area, field, mouse_col) {
            form.extend_mouse_selection(offset);
        }
    }

    fn handle_host_editor_left_release(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(form) = self.selected_host_editor_mut() else {
            return;
        };

        if let Some(field) = form.mouse_drag_field()
            && let Some(offset) = Self::host_editor_text_offset(form, inner_area, field, mouse_col)
        {
            form.extend_mouse_selection(offset);
        }

        form.finish_mouse_selection();
    }

    fn host_editor_item_at_point(&self, local_row: u16, mouse_col: u16, inner_area: Rect) -> Option<HostEditorVisibleItem> {
        let form = self.selected_host_editor()?;
        if local_row < 2 {
            return None;
        }

        let row_idx = local_row.saturating_sub(2) as usize;
        let visible_items = form.visible_items();
        let body_rows = visible_items
            .iter()
            .copied()
            .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
            .collect::<Vec<_>>();

        if row_idx < body_rows.len() {
            return body_rows.get(row_idx).copied();
        }

        if row_idx == body_rows.len().saturating_add(ACTION_ROW_OFFSET_AFTER_FIELDS) {
            let visible_fields = form.visible_fields();
            return Self::host_editor_action_hit(inner_area, mouse_col, visible_fields.contains(&HostEditorField::Delete));
        }

        None
    }

    fn host_editor_action_hit(inner_area: Rect, mouse_col: u16, include_delete: bool) -> Option<HostEditorVisibleItem> {
        let mut col = inner_area.x;
        let save_width = SAVE_LABEL.chars().count() as u16;
        if mouse_col >= col && mouse_col < col.saturating_add(save_width) {
            return Some(HostEditorVisibleItem::Field(HostEditorField::Save));
        }

        col = col.saturating_add(save_width).saturating_add(ACTION_SEPARATOR.chars().count() as u16);

        if include_delete {
            let delete_width = DELETE_LABEL.chars().count() as u16;
            if mouse_col >= col && mouse_col < col.saturating_add(delete_width) {
                return Some(HostEditorVisibleItem::Field(HostEditorField::Delete));
            }
            col = col.saturating_add(delete_width).saturating_add(ACTION_SEPARATOR.chars().count() as u16);
        }

        let cancel_width = CANCEL_LABEL.chars().count() as u16;
        if mouse_col >= col && mouse_col < col.saturating_add(cancel_width) {
            return Some(HostEditorVisibleItem::Field(HostEditorField::Cancel));
        }

        None
    }

    fn host_editor_text_offset(form: &HostEditorState, inner_area: Rect, field: HostEditorField, mouse_col: u16) -> Option<usize> {
        let editable = matches!(
            field,
            HostEditorField::Name
                | HostEditorField::Description
                | HostEditorField::Host
                | HostEditorField::User
                | HostEditorField::Port
                | HostEditorField::Profile
                | HostEditorField::VaultPass
                | HostEditorField::IdentityFile
                | HostEditorField::ProxyJump
                | HostEditorField::ProxyCommand
                | HostEditorField::ForwardAgent
                | HostEditorField::LocalForward
                | HostEditorField::RemoteForward
                | HostEditorField::SshOptions
                | HostEditorField::RdpDomain
                | HostEditorField::RdpArgs
        );
        if !editable {
            return None;
        }

        let start_col = inner_area.x.saturating_add(field.label().chars().count() as u16).saturating_add(2);
        let value_width = inner_area.width.saturating_sub(field.label().chars().count() as u16).saturating_sub(2);
        let scroll_offset = form.field_horizontal_scroll_offset(field, value_width);
        Some(scroll_offset.saturating_add(mouse_col.saturating_sub(start_col) as usize))
    }

    pub(crate) fn host_editor_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }
}
