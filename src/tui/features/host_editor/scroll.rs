//! Host editor body/footer/scrollbar layout helpers.

use crate::tui::{HostEditorState, HostEditorVisibleItem};
use ratatui::layout::Rect;

pub(crate) const EDITOR_FOOTER_LINE_COUNT: usize = 3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct EditorScrollbarGeometry {
    pub(crate) area: Rect,
    pub(crate) thumb_top: u16,
    pub(crate) thumb_height: u16,
    pub(crate) max_offset: usize,
}

pub(crate) fn body_items(form: &HostEditorState) -> Vec<HostEditorVisibleItem> {
    form.visible_items()
        .into_iter()
        .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
        .collect()
}

fn selected_body_row(form: &HostEditorState) -> Option<usize> {
    let items = body_items(form);
    let idx = items.iter().position(|item| *item == form.selected)?;
    // File row + spacer row precede body items.
    Some(idx.saturating_add(2))
}

pub(crate) fn footer_visible_lines(inner_height: u16) -> usize {
    EDITOR_FOOTER_LINE_COUNT.min(inner_height as usize)
}

pub(crate) fn body_viewport_height(inner_height: u16) -> usize {
    inner_height as usize - footer_visible_lines(inner_height)
}

pub(crate) fn body_scroll_offset(form: &HostEditorState, total_body_lines: usize, viewport_height: usize) -> usize {
    if viewport_height == 0 || total_body_lines <= viewport_height {
        return 0;
    }

    let max_offset = total_body_lines.saturating_sub(viewport_height);
    let mut scroll_offset = 0usize;
    if let Some(selected_row) = selected_body_row(form)
        && selected_row >= viewport_height
    {
        scroll_offset = selected_row.saturating_add(1).saturating_sub(viewport_height);
    }

    scroll_offset.min(max_offset)
}

pub(crate) fn scrollbar_geometry(inner_area: Rect, total_body_lines: usize, viewport_height: usize, scroll_offset: usize) -> Option<EditorScrollbarGeometry> {
    if inner_area.width < 2 || viewport_height == 0 || total_body_lines <= viewport_height {
        return None;
    }

    let area = Rect::new(
        inner_area.x.saturating_add(inner_area.width.saturating_sub(1)),
        inner_area.y,
        1,
        viewport_height as u16,
    );
    let scrollbar_height = area.height as usize;
    if scrollbar_height == 0 {
        return None;
    }

    let total_rows = total_body_lines.max(1);
    let max_offset = total_body_lines.saturating_sub(viewport_height);
    let thumb_height = (scrollbar_height.saturating_mul(viewport_height) / total_rows).max(1).min(scrollbar_height) as u16;
    let available_track = area.height.saturating_sub(thumb_height);
    let clamped_offset = scroll_offset.min(max_offset);
    let thumb_offset = if max_offset == 0 || available_track == 0 {
        0
    } else {
        ((available_track as usize).saturating_mul(clamped_offset) / max_offset) as u16
    };

    Some(EditorScrollbarGeometry {
        area,
        thumb_top: area.y.saturating_add(thumb_offset),
        thumb_height,
        max_offset,
    })
}

pub(crate) fn body_content_width(inner_width: u16, scrollbar: Option<EditorScrollbarGeometry>) -> u16 {
    if scrollbar.is_some() { inner_width.saturating_sub(1) } else { inner_width }
}

pub(crate) fn scroll_offset_from_scrollbar_row(geometry: EditorScrollbarGeometry, mouse_row: u16) -> usize {
    if geometry.area.height == 0 || geometry.max_offset == 0 {
        return 0;
    }

    let track_len = geometry.area.height.saturating_sub(1) as usize;
    if track_len == 0 {
        return 0;
    }

    let local_row = mouse_row.saturating_sub(geometry.area.y).min(geometry.area.height.saturating_sub(1)) as usize;
    ((local_row.saturating_mul(geometry.max_offset)) + (track_len / 2)) / track_len
}
