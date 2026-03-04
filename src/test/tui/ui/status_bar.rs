use super::AppState;
use crate::auth::ipc::VaultStatus;
use crate::inventory::TreeFolder;
use crate::tui::{HostTreeRow, HostTreeRowKind};
use ratatui::style::Modifier;
use ratatui::text::Span;
use std::path::PathBuf;

#[test]
fn calculates_span_width_using_unicode_display_width() {
    let spans = vec![Span::raw("a界"), Span::raw("x")];
    assert_eq!(AppState::spans_display_width(&spans), 4);
}

#[test]
fn manager_status_bar_shows_locked_vault_state() {
    let mut app = AppState::new_for_tests();
    app.host_tree_root.children.push(TreeFolder {
        id: 1,
        name: "folder".to_string(),
        path: PathBuf::from("~/.ssh/folder"),
        children: Vec::new(),
        host_indices: Vec::new(),
    });
    app.visible_host_rows = vec![HostTreeRow {
        kind: HostTreeRowKind::Folder(1),
        indent: String::new(),
        display_name: "folder".to_string(),
        expanded: true,
    }];
    app.selected_host_row = 0;
    app.vault_status = VaultStatus::locked(true);

    let (left, _) = app.build_manager_status_spans();
    let text: String = left.iter().map(|span| span.content.as_ref()).collect();

    assert_eq!(text, "Host ||  Vault || Folder: folder");
    assert_eq!(left[2].style.fg, Some(super::theme::ansi_red()));
    assert!(left[3].style.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn manager_status_bar_shows_unlocked_vault_state() {
    let mut app = AppState::new_for_tests();
    app.vault_status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };

    let (left, _) = app.build_manager_status_spans();
    let text: String = left.iter().map(|span| span.content.as_ref()).collect();

    assert_eq!(text, "Host ||  Vault || none");
    assert_eq!(left[2].style.fg, Some(super::theme::ansi_green()));
}
