//! Folder picker and folder-management mouse handling.

use crate::tui::text_edit;
use crate::tui::{AppState, FolderCreateState, FolderRenameState};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const FOLDER_RENAME_LABEL_PREFIX: &str = "New Name: ";
const FOLDER_CREATE_LABEL_PREFIX: &str = "Name: ";
const FOLDER_CREATE_PARENT_ROW_OFFSET: u16 = 0;
const FOLDER_CREATE_NAME_ROW_OFFSET: u16 = 1;
const FOLDER_CREATE_ACTION_ROW_OFFSET: u16 = 3;
const SELECT_ACTION_LABEL: &str = "[ Enter ] Select";
const SAVE_ACTION_LABEL: &str = "[ Enter ] Save";
const ACTION_SEPARATOR: &str = " | ";
const CANCEL_ACTION_LABEL: &str = "[ Esc ] Cancel";

impl AppState {
    pub(crate) fn folder_picker_modal_layout(&self) -> Option<(Rect, Rect)> {
        let picker = self.folder_picker.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let width = full_area.width.clamp(50, 92);
        let max_list_height = full_area.height.saturating_sub(8).max(4);
        let desired_list_height = (picker.rows.len() as u16).clamp(4, max_list_height);
        let height = desired_list_height.saturating_add(4).min(full_area.height);
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn folder_rename_modal_layout(&self) -> Option<(Rect, Rect)> {
        self.folder_rename.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let width = full_area.width.clamp(56, 88);
        let height = 8u16.min(full_area.height);
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn folder_create_modal_layout(&self) -> Option<(Rect, Rect)> {
        self.folder_create.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let width = full_area.width.clamp(46, 76);
        let height = 6u16.min(full_area.height);
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn folder_delete_confirm_modal_layout(&self) -> Option<(Rect, Rect)> {
        self.folder_delete_confirm.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        if full_area.width == 0 || full_area.height == 0 {
            return None;
        }

        let width = full_area.width.clamp(46, 80);
        let height = 6u16.min(full_area.height);
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        Some((area, inner))
    }

    pub(crate) fn handle_folder_picker_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((area, inner)) = self.folder_picker_modal_layout() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::folder_picker_point_in_rect(area, mouse.column, mouse.row) {
                    self.cancel_folder_picker();
                    return;
                }

                if !Self::folder_picker_point_in_rect(inner, mouse.column, mouse.row) {
                    return;
                }

                let action_row = inner.y.saturating_add(inner.height.saturating_sub(1));
                if mouse.row == action_row {
                    let select_start = inner.x;
                    let select_end = select_start.saturating_add(SELECT_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= select_start && mouse.column < select_end {
                        self.submit_folder_picker();
                        return;
                    }

                    let cancel_start = select_end.saturating_add(ACTION_SEPARATOR.chars().count() as u16);
                    let cancel_end = cancel_start.saturating_add(CANCEL_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= cancel_start && mouse.column < cancel_end {
                        self.cancel_folder_picker();
                    }
                    return;
                }

                if mouse.row <= inner.y {
                    return;
                }

                let list_height = inner.height.saturating_sub(2) as usize;
                if list_height == 0 {
                    return;
                }

                let selected = self.folder_picker.as_ref().map_or(0usize, |picker| picker.selected);
                let total = self.folder_picker.as_ref().map_or(0usize, |picker| picker.rows.len());
                let scroll_offset = Self::folder_picker_scroll_offset(selected, total, list_height);
                let local_row = mouse.row.saturating_sub(inner.y.saturating_add(1)) as usize;
                let row_idx = scroll_offset.saturating_add(local_row);

                if let Some(picker) = self.folder_picker.as_mut()
                    && row_idx < picker.rows.len()
                {
                    picker.selected = row_idx;
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(picker) = self.folder_picker.as_mut() {
                    picker.select_prev();
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(picker) = self.folder_picker.as_mut() {
                    picker.select_next();
                    self.mark_ui_dirty();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_folder_rename_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((area, inner)) = self.folder_rename_modal_layout() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::folder_picker_point_in_rect(area, mouse.column, mouse.row) {
                    self.folder_rename = None;
                    self.mark_ui_dirty();
                    return;
                }

                if !Self::folder_picker_point_in_rect(inner, mouse.column, mouse.row) {
                    return;
                }

                let action_row = inner.y.saturating_add(inner.height.saturating_sub(1));
                if mouse.row == action_row {
                    if let Some(state) = self.folder_rename.as_mut() {
                        state.drag_anchor = None;
                    }
                    let save_start = inner.x;
                    let save_end = save_start.saturating_add(SAVE_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= save_start && mouse.column < save_end {
                        self.submit_folder_rename();
                        return;
                    }

                    let cancel_start = save_end.saturating_add(ACTION_SEPARATOR.chars().count() as u16);
                    let cancel_end = cancel_start.saturating_add(CANCEL_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= cancel_start && mouse.column < cancel_end {
                        self.folder_rename = None;
                        self.mark_ui_dirty();
                    }
                    return;
                }

                let name_row = inner.y.saturating_add(1);
                if mouse.row == name_row
                    && let Some(state) = self.folder_rename.as_mut()
                    && let Some(cursor) = Self::folder_rename_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = None;
                    state.drag_anchor = Some(cursor);
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(state) = self.folder_rename.as_mut()
                    && let Some(anchor) = state.drag_anchor
                    && let Some(cursor) = Self::folder_rename_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = if cursor == anchor { None } else { Some((anchor, cursor)) };
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(state) = self.folder_rename.as_mut()
                    && let Some(anchor) = state.drag_anchor
                    && let Some(cursor) = Self::folder_rename_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = if cursor == anchor { None } else { Some((anchor, cursor)) };
                    state.drag_anchor = None;
                    self.mark_ui_dirty();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_folder_create_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((area, inner)) = self.folder_create_modal_layout() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::folder_picker_point_in_rect(area, mouse.column, mouse.row) {
                    self.folder_create = None;
                    self.mark_ui_dirty();
                    return;
                }

                if !Self::folder_picker_point_in_rect(inner, mouse.column, mouse.row) {
                    return;
                }

                let action_row = inner.y.saturating_add(FOLDER_CREATE_ACTION_ROW_OFFSET.min(inner.height.saturating_sub(1)));
                if mouse.row == action_row {
                    if let Some(state) = self.folder_create.as_mut() {
                        state.drag_anchor = None;
                    }
                    let save_start = inner.x;
                    let save_end = save_start.saturating_add(SAVE_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= save_start && mouse.column < save_end {
                        self.submit_folder_create();
                        return;
                    }

                    let cancel_start = save_end.saturating_add(ACTION_SEPARATOR.chars().count() as u16);
                    let cancel_end = cancel_start.saturating_add(CANCEL_ACTION_LABEL.chars().count() as u16);
                    if mouse.column >= cancel_start && mouse.column < cancel_end {
                        self.folder_create = None;
                        self.mark_ui_dirty();
                    }
                    return;
                }

                let parent_row = inner.y.saturating_add(FOLDER_CREATE_PARENT_ROW_OFFSET);
                if mouse.row == parent_row {
                    if let Some(state) = self.folder_create.as_mut() {
                        state.drag_anchor = None;
                    }
                    self.open_folder_picker_for_create_folder_parent();
                    return;
                }

                let name_row = inner.y.saturating_add(FOLDER_CREATE_NAME_ROW_OFFSET.min(inner.height.saturating_sub(1)));
                if mouse.row == name_row
                    && let Some(state) = self.folder_create.as_mut()
                    && let Some(cursor) = Self::folder_create_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = None;
                    state.drag_anchor = Some(cursor);
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(state) = self.folder_create.as_mut()
                    && let Some(anchor) = state.drag_anchor
                    && let Some(cursor) = Self::folder_create_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = if cursor == anchor { None } else { Some((anchor, cursor)) };
                    self.mark_ui_dirty();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(state) = self.folder_create.as_mut()
                    && let Some(anchor) = state.drag_anchor
                    && let Some(cursor) = Self::folder_create_cursor_from_column(state, inner, mouse.column)
                {
                    state.cursor = cursor;
                    state.selection = if cursor == anchor { None } else { Some((anchor, cursor)) };
                    state.drag_anchor = None;
                    self.mark_ui_dirty();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_folder_delete_confirm_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((area, _)) = self.folder_delete_confirm_modal_layout() else {
            return;
        };

        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind
            && !Self::folder_picker_point_in_rect(area, mouse.column, mouse.row)
        {
            self.folder_delete_confirm = None;
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn folder_picker_scroll_offset(selected: usize, total: usize, viewport: usize) -> usize {
        if total <= viewport || viewport == 0 {
            return 0;
        }

        let half = viewport / 2;
        let max_offset = total.saturating_sub(viewport);
        selected.saturating_sub(half).min(max_offset)
    }

    pub(crate) fn folder_picker_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }

    fn folder_rename_cursor_from_column(state: &FolderRenameState, inner: Rect, mouse_col: u16) -> Option<usize> {
        let value_start = inner.x.saturating_add(FOLDER_RENAME_LABEL_PREFIX.chars().count() as u16);
        let value_width = inner.width.saturating_sub(FOLDER_RENAME_LABEL_PREFIX.chars().count() as u16);
        if value_width == 0 {
            return Some(0);
        }
        let offset = mouse_col.saturating_sub(value_start) as usize;
        Some(offset.min(text_edit::char_len(&state.name)))
    }

    fn folder_create_cursor_from_column(state: &FolderCreateState, inner: Rect, mouse_col: u16) -> Option<usize> {
        let value_start = inner.x.saturating_add(FOLDER_CREATE_LABEL_PREFIX.chars().count() as u16);
        let value_width = inner.width.saturating_sub(FOLDER_CREATE_LABEL_PREFIX.chars().count() as u16);
        if value_width == 0 {
            return Some(0);
        }
        let offset = mouse_col.saturating_sub(value_start) as usize;
        Some(offset.min(text_edit::char_len(&state.name)))
    }
}
