//! Host editor mouse helpers.

use super::{HOST_DELETE_CONFIRM_ACTION_SEPARATOR, HOST_DELETE_CONFIRM_CANCEL_LABEL, HOST_DELETE_CONFIRM_DELETE_LABEL};
use crate::tui::features::host_editor::scroll::{
    EditorScrollbarGeometry, body_content_width as editor_body_content_width, body_items as host_editor_body_items,
    body_scroll_offset as host_editor_body_scroll_offset, body_viewport_height as host_editor_body_viewport_height, scroll_offset_from_scrollbar_row,
    scrollbar_geometry as host_editor_scrollbar_geometry,
};
use crate::tui::{AppState, HostEditorField, HostEditorState, HostEditorVisibleItem};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const SAVE_LABEL: &str = "[ Enter ] Save Entry";
const DELETE_LABEL: &str = "[ Ctrl+D ] Delete Entry";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const ACTION_SEPARATOR: &str = " | ";

impl AppState {
    pub(crate) fn host_delete_confirm_modal_layout(&self) -> Option<(Rect, Rect)> {
        self.host_delete_confirm.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let width = full_area.width.clamp(46, 80);
        let height = 5u16.min(full_area.height);
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn handle_host_delete_confirm_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((area, inner)) = self.host_delete_confirm_modal_layout() else {
            return;
        };

        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }

        if !Self::host_editor_point_in_rect(area, mouse.column, mouse.row) {
            self.host_delete_confirm = None;
            self.mark_ui_dirty();
            return;
        }

        if !Self::host_editor_point_in_rect(inner, mouse.column, mouse.row) {
            return;
        }

        let action_row = inner.y.saturating_add(inner.height.saturating_sub(1));
        if mouse.row != action_row {
            return;
        }

        let mut col = inner.x;
        let delete_width = HOST_DELETE_CONFIRM_DELETE_LABEL.chars().count() as u16;
        if mouse.column >= col && mouse.column < col.saturating_add(delete_width) {
            self.confirm_host_delete();
            return;
        }

        col = col
            .saturating_add(delete_width)
            .saturating_add(HOST_DELETE_CONFIRM_ACTION_SEPARATOR.chars().count() as u16);
        let cancel_width = HOST_DELETE_CONFIRM_CANCEL_LABEL.chars().count() as u16;
        if mouse.column >= col && mouse.column < col.saturating_add(cancel_width) {
            self.host_delete_confirm = None;
            self.mark_ui_dirty();
        }
    }

    fn host_editor_scrollbar_geometry(form: &HostEditorState, inner_area: Rect) -> Option<EditorScrollbarGeometry> {
        let body_items = host_editor_body_items(form);
        let total_body_lines = body_items.len().saturating_add(2);
        let viewport_height = host_editor_body_viewport_height(inner_area.height);
        let scroll_offset = host_editor_body_scroll_offset(form, total_body_lines, viewport_height);
        host_editor_scrollbar_geometry(inner_area, total_body_lines, viewport_height, scroll_offset)
    }

    fn host_editor_body_content_width(form: &HostEditorState, inner_area: Rect) -> u16 {
        editor_body_content_width(inner_area.width, Self::host_editor_scrollbar_geometry(form, inner_area))
    }

    fn host_editor_set_scroll_from_scrollbar_row(&mut self, mouse_row: u16, inner_area: Rect) {
        let Some(form) = self.selected_host_editor_mut() else {
            return;
        };

        let body_items = host_editor_body_items(form);
        if body_items.is_empty() {
            return;
        }

        let total_body_lines = body_items.len().saturating_add(2);
        let viewport_height = host_editor_body_viewport_height(inner_area.height);
        if viewport_height == 0 {
            return;
        }

        let current_offset = host_editor_body_scroll_offset(form, total_body_lines, viewport_height);
        let Some(scrollbar) = host_editor_scrollbar_geometry(inner_area, total_body_lines, viewport_height, current_offset) else {
            return;
        };

        let target_offset = scroll_offset_from_scrollbar_row(scrollbar, mouse_row).min(scrollbar.max_offset);
        let target_selected_row = target_offset.saturating_add(viewport_height.saturating_sub(1));
        let clamped_selected_row = target_selected_row.clamp(2, total_body_lines.saturating_sub(1));
        let item_idx = clamped_selected_row.saturating_sub(2).min(body_items.len().saturating_sub(1));
        form.selected = body_items[item_idx];
        form.finish_mouse_selection();
    }

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
                self.is_dragging_host_editor_scrollbar = false;

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

                if let Some(form) = self.selected_host_editor()
                    && let Some(scrollbar) = Self::host_editor_scrollbar_geometry(form, inner_area)
                    && Self::host_editor_point_in_rect(scrollbar.area, mouse.column, mouse.row)
                {
                    self.is_dragging_host_editor_scrollbar = true;
                    self.host_editor_set_scroll_from_scrollbar_row(mouse.row, inner_area);
                    return;
                }

                self.handle_host_editor_left_click(mouse.column, mouse.row, inner_area);
            }
            MouseEventKind::ScrollUp => {
                if let Some(form) = self.selected_host_editor_mut() {
                    form.select_prev_field();
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(form) = self.selected_host_editor_mut() {
                    form.select_next_field();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_dragging_host_editor_scrollbar {
                    self.host_editor_set_scroll_from_scrollbar_row(mouse.row, inner_area);
                    return;
                }
                self.handle_host_editor_left_drag(mouse.column, inner_area);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_dragging_host_editor_scrollbar {
                    self.is_dragging_host_editor_scrollbar = false;
                    return;
                }
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
                        let body_content_width = Self::host_editor_body_content_width(form, inner_area);
                        if let Some(offset) = Self::host_editor_text_offset(form, inner_area, body_content_width, field, mouse_col) {
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

        let body_content_width = Self::host_editor_body_content_width(form, inner_area);
        if let Some(offset) = Self::host_editor_text_offset(form, inner_area, body_content_width, field, mouse_col) {
            form.extend_mouse_selection(offset);
        }
    }

    fn handle_host_editor_left_release(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(form) = self.selected_host_editor_mut() else {
            return;
        };

        if let Some(field) = form.mouse_drag_field()
            && let Some(offset) = Self::host_editor_text_offset(form, inner_area, Self::host_editor_body_content_width(form, inner_area), field, mouse_col)
        {
            form.extend_mouse_selection(offset);
        }

        form.finish_mouse_selection();
    }

    fn host_editor_item_at_point(&self, local_row: u16, mouse_col: u16, inner_area: Rect) -> Option<HostEditorVisibleItem> {
        let form = self.selected_host_editor()?;
        if inner_area.height == 0 {
            return None;
        }

        let action_row = inner_area.height.saturating_sub(1) as usize;
        let local_row = local_row as usize;
        if local_row == action_row {
            let visible_fields = form.visible_fields();
            return Self::host_editor_action_hit(inner_area, mouse_col, visible_fields.contains(&HostEditorField::Delete));
        }

        let viewport_height = host_editor_body_viewport_height(inner_area.height);
        if viewport_height == 0 || local_row >= viewport_height {
            return None;
        }

        let body_items = host_editor_body_items(form);
        let total_body_lines = body_items.len().saturating_add(2);
        let scroll_offset = host_editor_body_scroll_offset(form, total_body_lines, viewport_height);
        let absolute_row = scroll_offset.saturating_add(local_row);
        if absolute_row < 2 {
            return None;
        }

        body_items.get(absolute_row.saturating_sub(2)).copied()
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

    fn host_editor_text_offset(form: &HostEditorState, inner_area: Rect, body_content_width: u16, field: HostEditorField, mouse_col: u16) -> Option<usize> {
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
        let value_width = body_content_width.saturating_sub(field.label().chars().count() as u16).saturating_sub(2);
        let scroll_offset = form.field_horizontal_scroll_offset(field, value_width);
        Some(scroll_offset.saturating_add(mouse_col.saturating_sub(start_col) as usize))
    }

    pub(crate) fn host_editor_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }
}
