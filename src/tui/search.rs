//! Fuzzy search and host filtering logic

use super::{HostTreeRow, HostTreeRowKind, SessionManager};
use crate::ssh_config::{FolderId, TreeFolder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostRowKey {
    Folder(FolderId),
    Host(usize),
}

/// Fuzzy match scoring for host search.
///
/// Returns a score if `pattern` fuzzy-matches against `text`, or `None` if
/// there is no match. Consecutive character matches score higher.
pub fn fuzzy_match(text: &str, pattern: &str) -> Option<i32> {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();

    let mut text_chars = text.chars().peekable();
    let mut pattern_chars = pattern.chars().peekable();
    let mut score = 0;
    let mut consecutive = 0;

    while let Some(&pattern_char) = pattern_chars.peek() {
        let mut found = false;

        while let Some(&text_char) = text_chars.peek() {
            text_chars.next();
            if text_char == pattern_char {
                score += 1 + consecutive;
                consecutive += 1;
                pattern_chars.next();
                found = true;
                break;
            } else {
                consecutive = 0;
            }
        }

        if !found {
            return None; // Pattern not found
        }
    }

    Some(score)
}

/// Strict contiguous match scoring.
///
/// Higher score for prefix matches, then earlier substring positions.
fn strict_match_score(text: &str, pattern: &str) -> Option<i32> {
    let text = text.to_lowercase();
    let pos = text.find(pattern)?;

    if pos == 0 {
        Some(300 + pattern.len() as i32)
    } else {
        Some((200 - pos as i32).max(1))
    }
}

impl SessionManager {
    fn row_key_from_kind(kind: HostTreeRowKind) -> HostRowKey {
        match kind {
            HostTreeRowKind::Folder(id) => HostRowKey::Folder(id),
            HostTreeRowKind::Host(idx) => HostRowKey::Host(idx),
        }
    }

    fn selected_row_key(&self) -> Option<HostRowKey> {
        self.visible_host_rows.get(self.selected_host_row).map(|row| Self::row_key_from_kind(row.kind))
    }

    fn rebuild_visible_host_rows(&mut self) {
        let mut rows = Vec::new();
        if self.search_query.is_empty() {
            self.collect_root_visible_rows_normal(&mut rows);
        } else {
            self.collect_root_visible_rows_search(&mut rows);
        }
        self.visible_host_rows = rows;
    }

    /// Build visible rows in normal mode while keeping the synthetic root hidden.
    fn collect_root_visible_rows_normal(&self, rows: &mut Vec<HostTreeRow>) {
        for &host_idx in &self.host_tree_root.host_indices {
            if let Some(host) = self.hosts.get(host_idx) {
                rows.push(HostTreeRow {
                    kind: HostTreeRowKind::Host(host_idx),
                    depth: 0,
                    display_name: host.name.clone(),
                    expanded: false,
                });
            }
        }

        for child in &self.host_tree_root.children {
            self.collect_visible_rows_normal(child, 0, rows);
        }
    }

    /// Build visible rows in search mode while keeping the synthetic root hidden.
    fn collect_root_visible_rows_search(&self, rows: &mut Vec<HostTreeRow>) {
        for &host_idx in &self.host_tree_root.host_indices {
            if self.host_match_scores.contains_key(&host_idx)
                && let Some(host) = self.hosts.get(host_idx)
            {
                rows.push(HostTreeRow {
                    kind: HostTreeRowKind::Host(host_idx),
                    depth: 0,
                    display_name: host.name.clone(),
                    expanded: false,
                });
            }
        }

        for child in &self.host_tree_root.children {
            let mut sub_rows = Vec::new();
            if self.collect_visible_rows_search(child, 0, &mut sub_rows) {
                rows.extend(sub_rows);
            }
        }
    }

    fn collect_visible_rows_normal(&self, folder: &TreeFolder, depth: usize, rows: &mut Vec<HostTreeRow>) {
        let expanded = !self.collapsed_folders.contains(&folder.id);
        rows.push(HostTreeRow {
            kind: HostTreeRowKind::Folder(folder.id),
            depth,
            display_name: folder.name.clone(),
            expanded,
        });

        if !expanded {
            return;
        }

        for &host_idx in &folder.host_indices {
            if let Some(host) = self.hosts.get(host_idx) {
                rows.push(HostTreeRow {
                    kind: HostTreeRowKind::Host(host_idx),
                    depth: depth + 1,
                    display_name: host.name.clone(),
                    expanded: false,
                });
            }
        }

        for child in &folder.children {
            self.collect_visible_rows_normal(child, depth + 1, rows);
        }
    }

    /// Collect search-filtered rows and return whether this folder has a matching descendant.
    fn collect_visible_rows_search(&self, folder: &TreeFolder, depth: usize, rows: &mut Vec<HostTreeRow>) -> bool {
        let mut child_rows = Vec::new();
        let mut has_child_match = false;
        for child in &folder.children {
            let mut sub_rows = Vec::new();
            let child_has = self.collect_visible_rows_search(child, depth + 1, &mut sub_rows);
            if child_has {
                has_child_match = true;
                child_rows.extend(sub_rows);
            }
        }

        let mut host_rows = Vec::new();
        for &host_idx in &folder.host_indices {
            if self.host_match_scores.contains_key(&host_idx)
                && let Some(host) = self.hosts.get(host_idx)
            {
                host_rows.push(HostTreeRow {
                    kind: HostTreeRowKind::Host(host_idx),
                    depth: depth + 1,
                    display_name: host.name.clone(),
                    expanded: false,
                });
            }
        }

        let has_match = !host_rows.is_empty() || has_child_match;
        if has_match {
            rows.push(HostTreeRow {
                kind: HostTreeRowKind::Folder(folder.id),
                depth,
                display_name: folder.name.clone(),
                expanded: true,
            });
            rows.extend(host_rows);
            rows.extend(child_rows);
        }

        has_match
    }

    fn sync_host_row_selection_state(&mut self) {
        if self.visible_host_rows.is_empty() {
            self.selected_host_row = 0;
            self.host_list_state.select(None);
            return;
        }

        if self.selected_host_row >= self.visible_host_rows.len() {
            self.selected_host_row = self.visible_host_rows.len().saturating_sub(1);
        }
        self.host_list_state.select(Some(self.selected_host_row));
    }

    fn repair_selection_after_rebuild(&mut self, preferred: Option<HostRowKey>) {
        if self.visible_host_rows.is_empty() {
            self.selected_host_row = 0;
            self.host_list_state.select(None);
            self.host_scroll_offset = 0;
            return;
        }

        if let Some(key) = preferred
            && let Some(idx) = self.visible_host_rows.iter().position(|row| Self::row_key_from_kind(row.kind) == key)
        {
            self.selected_host_row = idx;
            self.sync_host_row_selection_state();
            return;
        }

        self.selected_host_row = 0;
        self.sync_host_row_selection_state();
    }

    /// Update the filtered hosts based on search query with fuzzy matching.
    pub(super) fn update_filtered_hosts(&mut self) {
        let previous = self.selected_row_key();

        self.host_match_scores.clear();
        if !self.search_query.is_empty() {
            let query = self.search_query.to_lowercase();

            // Pass 1: strict contiguous matching.
            // If we have strict hits, only show those for more predictable filtering.
            for (idx, host) in self.hosts.iter().enumerate() {
                let mut best_score = None;

                if let Some(score) = strict_match_score(&host.name, &query) {
                    best_score = Some(score + 1000); // Strong name preference
                }

                if let Some(hostname) = &host.hostname
                    && let Some(score) = strict_match_score(hostname, &query)
                {
                    best_score = Some(best_score.unwrap_or(0).max(score + 500));
                }

                if let Some(user) = &host.user
                    && let Some(score) = strict_match_score(user, &query)
                {
                    best_score = Some(best_score.unwrap_or(0).max(score + 300));
                }

                if let Some(score) = best_score {
                    self.host_match_scores.insert(idx, score);
                }
            }

            if self.host_match_scores.is_empty() {
                // Pass 2: fallback to fuzzy matching only when strict matching found nothing.
                for (idx, host) in self.hosts.iter().enumerate() {
                    let mut best_score = None;

                    // Try matching against name
                    if let Some(score) = fuzzy_match(&host.name, &query) {
                        best_score = Some(score + 100); // Boost name matches
                    }

                    // Try matching against hostname
                    if let Some(hostname) = &host.hostname
                        && let Some(score) = fuzzy_match(hostname, &query)
                    {
                        best_score = Some(best_score.unwrap_or(0).max(score + 50));
                    }

                    // Try matching against user
                    if let Some(user) = &host.user
                        && let Some(score) = fuzzy_match(user, &query)
                    {
                        best_score = Some(best_score.unwrap_or(0).max(score + 30));
                    }

                    if let Some(score) = best_score {
                        self.host_match_scores.insert(idx, score);
                    }
                }
            }
        }

        self.rebuild_visible_host_rows();
        self.host_scroll_offset = 0;
        self.repair_selection_after_rebuild(previous);
    }

    pub(super) fn selected_host_idx(&self) -> Option<usize> {
        match self.visible_host_rows.get(self.selected_host_row) {
            Some(HostTreeRow {
                kind: HostTreeRowKind::Host(host_idx),
                ..
            }) => Some(*host_idx),
            _ => None,
        }
    }

    pub(super) fn selected_folder_id(&self) -> Option<FolderId> {
        match self.visible_host_rows.get(self.selected_host_row) {
            Some(HostTreeRow {
                kind: HostTreeRowKind::Folder(folder_id),
                ..
            }) => Some(*folder_id),
            _ => None,
        }
    }

    pub(super) fn set_selected_row(&mut self, row: usize) {
        self.selected_host_row = row;
        self.sync_host_row_selection_state();
    }

    pub(super) fn visible_host_row_count(&self) -> usize {
        self.visible_host_rows.len()
    }

    pub(super) fn matched_host_count(&self) -> usize {
        self.host_match_scores.len()
    }

    pub(super) fn is_folder_expanded(&self, folder_id: FolderId) -> bool {
        if !self.search_query.is_empty() {
            return true;
        }
        !self.collapsed_folders.contains(&folder_id)
    }

    pub(super) fn set_folder_expanded(&mut self, folder_id: FolderId, expanded: bool) {
        if !self.search_query.is_empty() {
            return;
        }

        if expanded {
            self.collapsed_folders.remove(&folder_id);
        } else {
            self.collapsed_folders.insert(folder_id);
        }

        self.rebuild_visible_host_rows();
        self.repair_selection_after_rebuild(Some(HostRowKey::Folder(folder_id)));
    }

    pub(super) fn toggle_folder(&mut self, folder_id: FolderId) {
        let expanded = self.is_folder_expanded(folder_id);
        self.set_folder_expanded(folder_id, !expanded);
    }

    fn folder_by_id_recursive(folder: &TreeFolder, folder_id: FolderId) -> Option<&TreeFolder> {
        if folder.id == folder_id {
            return Some(folder);
        }
        for child in &folder.children {
            if let Some(found) = Self::folder_by_id_recursive(child, folder_id) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn folder_by_id(&self, folder_id: FolderId) -> Option<&TreeFolder> {
        Self::folder_by_id_recursive(&self.host_tree_root, folder_id)
    }

    fn count_hosts_recursive(folder: &TreeFolder) -> usize {
        let mut count = folder.host_indices.len();
        for child in &folder.children {
            count += Self::count_hosts_recursive(child);
        }
        count
    }

    pub(super) fn folder_descendant_host_count(&self, folder_id: FolderId) -> usize {
        self.folder_by_id(folder_id).map(Self::count_hosts_recursive).unwrap_or(0)
    }

    /// Update host list scroll to keep selection visible.
    pub(super) fn update_host_scroll(&mut self, viewport_height: usize) {
        if self.visible_host_rows.is_empty() || viewport_height == 0 {
            return;
        }

        // Symmetric scrolling: keep cursor in viewport before scrolling.
        if self.selected_host_row < self.host_scroll_offset {
            self.host_scroll_offset = self.selected_host_row;
        } else if self.selected_host_row >= self.host_scroll_offset + viewport_height {
            self.host_scroll_offset = self.selected_host_row.saturating_sub(viewport_height - 1);
        }
    }
}
