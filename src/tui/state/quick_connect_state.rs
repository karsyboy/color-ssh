//! Quick-connect form state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectField {
    User,
    Host,
    Profile,
    Logging,
    Connect,
    Cancel,
}

impl QuickConnectField {
    // Focus traversal helpers.
    pub(crate) fn next(self) -> Self {
        match self {
            Self::User => Self::Host,
            Self::Host => Self::Profile,
            Self::Profile => Self::Logging,
            Self::Logging => Self::Connect,
            Self::Connect => Self::Cancel,
            Self::Cancel => Self::User,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            Self::User => Self::Cancel,
            Self::Host => Self::User,
            Self::Profile => Self::Host,
            Self::Logging => Self::Profile,
            Self::Connect => Self::Logging,
            Self::Cancel => Self::Connect,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QuickConnectState {
    pub(crate) user: String,
    pub(crate) user_cursor: usize,
    pub(crate) user_selection: Option<(usize, usize)>,
    pub(crate) host: String,
    pub(crate) host_cursor: usize,
    pub(crate) host_selection: Option<(usize, usize)>,
    drag_anchor: Option<(QuickConnectField, usize)>,
    pub(crate) profile_options: Vec<String>,
    pub(crate) profile_index: usize,
    pub(crate) ssh_logging: bool,
    pub(crate) selected: QuickConnectField,
    pub(crate) host_required: bool,
}

impl QuickConnectState {
    // Construction.
    pub(crate) fn new(default_ssh_logging: bool, mut profile_options: Vec<String>) -> Self {
        if profile_options.is_empty() {
            profile_options.push("default".to_string());
        }
        let profile_index = profile_options.iter().position(|profile| profile.eq_ignore_ascii_case("default")).unwrap_or(0);

        Self {
            user: String::new(),
            user_cursor: 0,
            user_selection: None,
            host: String::new(),
            host_cursor: 0,
            host_selection: None,
            drag_anchor: None,
            profile_options,
            profile_index,
            ssh_logging: default_ssh_logging,
            selected: QuickConnectField::User,
            host_required: false,
        }
    }

    // Text-cursor editing helpers.
    fn text_cursor_selection_mut(&mut self, field: QuickConnectField) -> Option<(&mut String, &mut usize, &mut Option<(usize, usize)>)> {
        match field {
            QuickConnectField::User => Some((&mut self.user, &mut self.user_cursor, &mut self.user_selection)),
            QuickConnectField::Host => Some((&mut self.host, &mut self.host_cursor, &mut self.host_selection)),
            _ => None,
        }
    }

    fn text_cursor_selection(&self, field: QuickConnectField) -> Option<(&str, usize, Option<(usize, usize)>)> {
        match field {
            QuickConnectField::User => Some((self.user.as_str(), self.user_cursor, self.user_selection)),
            QuickConnectField::Host => Some((self.host.as_str(), self.host_cursor, self.host_selection)),
            _ => None,
        }
    }

    fn char_len(text: &str) -> usize {
        text.chars().count()
    }

    fn clamp_cursor(text: &str, cursor: &mut usize) {
        *cursor = (*cursor).min(Self::char_len(text));
    }

    fn normalized_selection(text: &str, selection: Option<(usize, usize)>) -> Option<(usize, usize)> {
        let (start, end) = selection?;
        let len = Self::char_len(text);
        let start = start.min(len);
        let end = end.min(len);
        if start == end {
            None
        } else if start < end {
            Some((start, end))
        } else {
            Some((end, start))
        }
    }

    fn byte_index_for_char(text: &str, char_index: usize) -> usize {
        if char_index == 0 {
            return 0;
        }

        let max = Self::char_len(text);
        let clamped = char_index.min(max);
        if clamped == max {
            return text.len();
        }

        text.char_indices().nth(clamped).map_or(text.len(), |(byte_index, _)| byte_index)
    }

    pub(crate) fn begin_mouse_selection(&mut self, field: QuickConnectField, column: usize) {
        self.drag_anchor = None;
        let mut anchor = None;
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let next_cursor = column.min(Self::char_len(text));
            *cursor = next_cursor;
            *selection = None;
            anchor = Some(next_cursor);
        }
        if let Some(anchor) = anchor {
            self.drag_anchor = Some((field, anchor));
        }
    }

    pub(crate) fn extend_mouse_selection(&mut self, column: usize) {
        let Some((field, anchor)) = self.drag_anchor else {
            return;
        };

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let next_cursor = column.min(Self::char_len(text));
            *cursor = next_cursor;
            *selection = if next_cursor == anchor { None } else { Some((anchor, next_cursor)) };
        }
    }

    pub(crate) fn mouse_drag_field(&self) -> Option<QuickConnectField> {
        self.drag_anchor.map(|(field, _)| field)
    }

    pub(crate) fn finish_mouse_selection(&mut self) {
        self.drag_anchor = None;
    }

    pub(crate) fn move_cursor_left(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            Self::clamp_cursor(text, cursor);
            let active_selection = Self::normalized_selection(text, *selection);
            *selection = None;
            if let Some((start, _)) = active_selection {
                *cursor = start;
            } else if *cursor > 0 {
                *cursor -= 1;
            }
        }
    }

    pub(crate) fn move_cursor_right(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            Self::clamp_cursor(text, cursor);
            let len = Self::char_len(text);
            let active_selection = Self::normalized_selection(text, *selection);
            *selection = None;
            if let Some((_, end)) = active_selection {
                *cursor = end;
            } else if *cursor < len {
                *cursor += 1;
            }
        }
    }

    pub(crate) fn move_cursor_home(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if let Some((_, cursor, selection)) = self.text_cursor_selection_mut(field) {
            *cursor = 0;
            *selection = None;
        }
    }

    pub(crate) fn move_cursor_end(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            *cursor = Self::char_len(text);
            *selection = None;
        }
    }

    pub(crate) fn select_all(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            let len = Self::char_len(text);
            if len == 0 {
                *selection = None;
                *cursor = 0;
            } else {
                *selection = Some((0, len));
                *cursor = len;
            }
        }
    }

    fn delete_selection(&mut self, field: QuickConnectField) -> bool {
        let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) else {
            return false;
        };

        let Some((start, end)) = Self::normalized_selection(text, *selection) else {
            *selection = None;
            return false;
        };

        let start_byte = Self::byte_index_for_char(text, start);
        let end_byte = Self::byte_index_for_char(text, end);
        text.replace_range(start_byte..end_byte, "");
        *cursor = start;
        *selection = None;
        true
    }

    pub(crate) fn insert_char(&mut self, field: QuickConnectField, ch: char) {
        self.finish_mouse_selection();
        let _ = self.delete_selection(field);
        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            Self::clamp_cursor(text, cursor);
            let insert_at = Self::byte_index_for_char(text, *cursor);
            text.insert(insert_at, ch);
            *cursor += 1;
            *selection = None;
        }
    }

    pub(crate) fn backspace(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if self.delete_selection(field) {
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            Self::clamp_cursor(text, cursor);
            if *cursor == 0 {
                *selection = None;
                return;
            }

            let end = Self::byte_index_for_char(text, *cursor);
            let start = Self::byte_index_for_char(text, *cursor - 1);
            text.replace_range(start..end, "");
            *cursor -= 1;
            *selection = None;
        }
    }

    pub(crate) fn delete(&mut self, field: QuickConnectField) {
        self.finish_mouse_selection();
        if self.delete_selection(field) {
            return;
        }

        if let Some((text, cursor, selection)) = self.text_cursor_selection_mut(field) {
            Self::clamp_cursor(text, cursor);
            let len = Self::char_len(text);
            if *cursor >= len {
                *selection = None;
                return;
            }

            let start = Self::byte_index_for_char(text, *cursor);
            let end = Self::byte_index_for_char(text, *cursor + 1);
            text.replace_range(start..end, "");
            *selection = None;
        }
    }

    pub(crate) fn cursor_for_field(&self, field: QuickConnectField) -> Option<usize> {
        self.text_cursor_selection(field).map(|(_, cursor, _)| cursor)
    }

    pub(crate) fn selection_for_field(&self, field: QuickConnectField) -> Option<(usize, usize)> {
        self.text_cursor_selection(field)
            .and_then(|(text, _, selection)| Self::normalized_selection(text, selection))
    }

    // Profile accessors.
    pub(crate) fn selected_profile_label(&self) -> &str {
        self.profile_options.get(self.profile_index).map(String::as_str).unwrap_or("default")
    }

    pub(crate) fn selected_profile_for_cli(&self) -> Option<String> {
        let profile = self.selected_profile_label();
        if profile.eq_ignore_ascii_case("default") {
            None
        } else {
            Some(profile.to_string())
        }
    }

    // Profile selection changes.
    pub(crate) fn select_next_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.profile_options.len();
    }

    pub(crate) fn select_prev_profile(&mut self) {
        self.finish_mouse_selection();
        if self.profile_options.is_empty() {
            return;
        }
        if self.profile_index == 0 {
            self.profile_index = self.profile_options.len() - 1;
        } else {
            self.profile_index -= 1;
        }
    }
}
