use super::*;
use crate::inventory::build_inventory_tree;
use crate::test::support::fs::TestWorkspace;
use crate::tui::{HostEditorField, HostEditorMode, HostEditorState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::fs;
use std::path::{Path, PathBuf};

fn seed_app_from_inventory(app: &mut AppState, inventory_path: &Path) {
    let tree = build_inventory_tree(inventory_path).expect("load inventory tree");
    app.hosts = tree.hosts;
    app.host_tree_root = tree.root;
    app.host_search_index = AppState::build_host_search_index(&app.hosts);
    app.collapsed_folders.clear();
    app.inventory_load_error = None;
    app.search_query.clear();
    app.update_filtered_hosts();
    app.focus_on_manager = true;
}

fn find_host_row(app: &AppState, host_name: &str) -> usize {
    app.visible_host_rows
        .iter()
        .position(|row| {
            if let crate::tui::HostTreeRowKind::Host(host_idx) = row.kind {
                return app.hosts.get(host_idx).is_some_and(|host| host.name == host_name);
            }
            false
        })
        .unwrap_or_else(|| panic!("host row '{host_name}' not found"))
}

#[test]
fn edit_entry_updates_inventory_and_reloads_host_view() {
    let workspace = TestWorkspace::new("tui", "host_editor_edit").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("open edit modal");

    let form = app.host_editor.as_mut().expect("host editor state");
    form.host.value = "alpha-updated.example".to_string();

    app.submit_host_editor();

    let host = app.hosts.iter().find(|host| host.name == "alpha").expect("updated host");
    assert_eq!(host.host, "alpha-updated.example");

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("alpha-updated.example"));
}

#[test]
fn delete_entry_requires_confirmation_and_supports_cancel() {
    let workspace = TestWorkspace::new("tui", "host_editor_delete").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("open edit modal");

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Delete;
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.host_delete_confirm.as_ref().map(|confirm| confirm.host_name.as_str()), Some("alpha"));

    app.handle_host_delete_confirm_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.host_delete_confirm.is_none());
    assert!(app.hosts.iter().any(|host| host.name == "alpha"));

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_host_delete_confirm_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert!(app.hosts.iter().all(|host| host.name != "alpha"));
    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(!rendered.contains("name: alpha"));
}

#[test]
fn create_new_entry_with_keyboard_shortcut_saves_inventory() {
    let workspace = TestWorkspace::new("tui", "host_editor_create").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: existing
    protocol: ssh
    host: existing.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
        .expect("open create modal");

    {
        let form = app.host_editor.as_mut().expect("create host editor state");
        assert_eq!(form.mode, crate::tui::HostEditorMode::Create);
        assert_eq!(form.source_file, inventory_path);
        form.name.value = "new-host".to_string();
        form.host.value = "new-host.example".to_string();
    }

    app.submit_host_editor();

    let host = app.hosts.iter().find(|host| host.name == "new-host").expect("new host present");
    assert_eq!(host.profile, None);
    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("name: new-host"));
    assert!(!rendered.contains("profile: default"));
}

#[test]
fn edit_entry_selecting_default_profile_removes_profile_key() {
    let workspace = TestWorkspace::new("tui", "host_editor_profile_default_edit").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: alpha.example
    profile: linux
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("open edit modal");

    let form = app.host_editor.as_mut().expect("host editor state");
    form.profile.value = "default".to_string();

    app.submit_host_editor();

    let host = app.hosts.iter().find(|host| host.name == "alpha").expect("updated host");
    assert_eq!(host.profile, None);

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(!rendered.contains("profile:"));
}

#[test]
fn create_new_entry_folder_path_places_host_under_nested_folder() {
    let workspace = TestWorkspace::new("tui", "host_editor_folder_path").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: existing
    protocol: ssh
    host: existing.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.open_host_editor_for_new_entry(inventory_path.clone());

    {
        let form = app.host_editor.as_mut().expect("create host editor state");
        form.name.value = "nested-host".to_string();
        form.host.value = "nested.example".to_string();
        form.folder_path.value = "/folder/folder1/".to_string();
    }

    app.submit_host_editor();

    let host = app.hosts.iter().find(|host| host.name == "nested-host").expect("nested host present");
    assert_eq!(host.source_folder_path, vec!["folder".to_string(), "folder1".to_string()]);
}

#[test]
fn right_click_host_and_empty_space_open_expected_context_actions_and_navigation_still_works() {
    let workspace = TestWorkspace::new("tui", "host_editor_context").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: alpha.example
  - name: beta
    protocol: ssh
    host: beta.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.host_panel_visible = true;
    app.host_list_area = Rect::new(0, 0, 30, 8);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 1,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("right click host row");

    let menu = app.host_context_menu.as_ref().expect("host context menu");
    assert_eq!(menu.actions, vec![crate::tui::HostContextMenuAction::EditEntry]);

    app.host_context_menu = None;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 1,
        row: 7,
        modifiers: KeyModifiers::NONE,
    })
    .expect("right click empty host area");

    let menu = app.host_context_menu.as_ref().expect("empty context menu");
    assert_eq!(menu.actions, vec![crate::tui::HostContextMenuAction::NewEntry]);

    let row_before = app.selected_host_row;
    app.handle_manager_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).expect("manager down");
    assert!(app.selected_host_row >= row_before);
}

#[test]
fn protocol_field_cycles_with_arrow_keys_and_filters_visible_fields_by_protocol() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string(), "work".to_string()],
        vec!["rdp_lab".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Protocol;
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.protocol.value, "rdp");
    assert!(!form.visible_fields().contains(&HostEditorField::Profile));
    assert!(!form.visible_fields().contains(&HostEditorField::IdentityFile));
    assert!(form.visible_fields().contains(&HostEditorField::RdpDomain));
    assert!(form.visible_fields().contains(&HostEditorField::RdpArgs));
    assert!(!form.visible_fields().contains(&HostEditorField::Hidden));

    let form = app.host_editor.as_mut().expect("host editor state");
    form.selected = HostEditorField::Protocol;
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.protocol.value, "ssh");
    assert!(form.visible_fields().contains(&HostEditorField::Profile));
    assert!(form.visible_fields().contains(&HostEditorField::IdentityFile));
    assert!(!form.visible_fields().contains(&HostEditorField::RdpDomain));
}

#[test]
fn profile_field_cycles_with_arrow_keys() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string(), "work".to_string()],
        Vec::new(),
    ));

    let form = app.host_editor.as_mut().expect("host editor state");
    form.selected = HostEditorField::Profile;
    assert_eq!(form.mode, HostEditorMode::Create);
    assert_eq!(form.profile.value, "default");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.host_editor.as_ref().expect("host editor state").profile.value.as_str(), "work");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.host_editor.as_ref().expect("host editor state").profile.value.as_str(), "default");
}

#[test]
fn vault_pass_field_cycles_with_arrow_keys() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string(), "rdp_lab".to_string()],
    ));

    let form = app.host_editor.as_mut().expect("host editor state");
    form.selected = HostEditorField::VaultPass;
    assert_eq!(form.vault_pass.value, "db_prod");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.host_editor.as_ref().expect("host editor state").vault_pass.value.as_str(), "rdp_lab");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.host_editor.as_ref().expect("host editor state").vault_pass.value.as_str(), "db_prod");
}

#[test]
fn selected_form_fields_do_not_show_examples() {
    let form = HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]);
    assert!(form.field_example(HostEditorField::Name).is_none());
    assert!(form.field_example(HostEditorField::Description).is_none());
    assert!(form.field_example(HostEditorField::Protocol).is_none());
    assert!(form.field_example(HostEditorField::Host).is_none());
    assert!(form.field_example(HostEditorField::User).is_none());
    assert!(form.field_example(HostEditorField::Profile).is_none());
    assert!(form.field_example(HostEditorField::VaultPass).is_none());
    assert!(form.field_example(HostEditorField::RdpDomain).is_none());

    assert!(form.field_example(HostEditorField::Port).is_some());
    assert!(form.field_example(HostEditorField::IdentityFile).is_some());
    assert!(form.field_example(HostEditorField::FolderPath).is_some());
}

#[test]
fn protocol_switch_sets_default_ports_for_ssh_and_rdp() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        assert_eq!(form.port.value, "22");
        form.selected = HostEditorField::Protocol;
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    {
        let form = app.host_editor.as_ref().expect("host editor state");
        assert_eq!(form.protocol.value, "rdp");
        assert_eq!(form.port.value, "3389");
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    {
        let form = app.host_editor.as_ref().expect("host editor state");
        assert_eq!(form.protocol.value, "ssh");
        assert_eq!(form.port.value, "22");
    }

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.port.value = "3390".to_string();
        form.selected = HostEditorField::Protocol;
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.protocol.value, "rdp");
    assert_eq!(form.port.value, "3390");
}

#[test]
fn description_field_accepts_space_input() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Description;
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.description.value, "a b");
}

#[test]
fn non_description_fields_ignore_space_input() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Host;
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.host.value, "ab");
}

#[test]
fn host_editor_paste_preserves_spaces_only_for_description() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Host;
    }
    app.handle_host_editor_paste("with spaces");
    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.host.value, "withspaces");

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Description;
    }
    app.handle_host_editor_paste("more spaces");
    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.description.value, "more spaces");
}

#[test]
fn identities_only_space_still_cycles_value() {
    let mut app = AppState::new_for_tests();
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::IdentitiesOnly;
        assert_eq!(form.identities_only_display(), "auto");
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.identities_only_display(), "yes");
}

#[test]
fn host_editor_mouse_selection_allows_drag_highlight_and_delete() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 40);
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    {
        let form = app.host_editor.as_mut().expect("host editor state");
        form.host.value = "alpha.example".to_string();
        form.host.cursor = form.host.value.chars().count();
    }

    let (_, inner) = app.host_editor_modal_layout().expect("host editor layout");
    let host_row_index = app
        .host_editor
        .as_ref()
        .expect("host editor state")
        .visible_fields()
        .iter()
        .position(|field| *field == HostEditorField::Host)
        .expect("host field row index");
    let row = inner.y.saturating_add(2).saturating_add(host_row_index as u16);
    let text_start_col = inner.x.saturating_add("Host: ".chars().count() as u16);

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: text_start_col,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse down on host field");
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: text_start_col.saturating_add(5),
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse drag on host field");
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: text_start_col.saturating_add(5),
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse release on host field");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.selected, HostEditorField::Host);
    assert_eq!(form.host.value, ".example");
}

#[test]
fn host_editor_bottom_action_row_supports_mouse_clicks() {
    let workspace = TestWorkspace::new("tui", "host_editor_mouse_actions").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: alpha
    protocol: ssh
    host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 40);
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));
    app.handle_manager_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
        .expect("open edit modal");

    let (_, inner) = app.host_editor_modal_layout().expect("host editor layout");
    let action_row = {
        let form = app.host_editor.as_ref().expect("host editor state");
        let non_action_rows = form.visible_fields().iter().filter(|field| !field.is_action()).count() as u16;
        inner.y.saturating_add(2).saturating_add(non_action_rows).saturating_add(3)
    };

    let delete_col = inner.x.saturating_add(("[ Enter ] Save Entry | ".chars().count()) as u16);
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: delete_col,
        row: action_row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click delete action");
    assert_eq!(app.host_delete_confirm.as_ref().map(|confirm| confirm.host_name.as_str()), Some("alpha"));

    app.handle_host_delete_confirm_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.host_delete_confirm.is_none());

    let cancel_col = inner.x.saturating_add(("[ Enter ] Save Entry | [ d ] Delete Entry | ".chars().count()) as u16);
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: cancel_col,
        row: action_row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click cancel action");
    assert!(app.host_editor.is_none());
}

#[test]
fn host_editor_modal_height_includes_bottom_action_row() {
    let form = HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]);
    let non_action_rows = form.visible_fields().iter().filter(|field| !field.is_action()).count() as u16;
    let rendered_lines = 2 + non_action_rows + 1 + 1 + 1 + 1;
    let inner_height = form.modal_height().saturating_sub(2);

    assert!(
        inner_height >= rendered_lines,
        "modal inner height {inner_height} must fit rendered host-editor lines {rendered_lines}"
    );
}

#[test]
fn selected_field_scrolls_horizontally_when_cursor_moves_past_visible_width() {
    let mut form = HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]);
    form.selected = HostEditorField::Host;
    form.host.value = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
    form.host.cursor = form.host.value.chars().count();

    let scroll = form.field_horizontal_scroll_offset(HostEditorField::Host, 16);
    assert!(scroll > 0, "expected non-zero horizontal scroll for long value at end cursor");
}

#[test]
fn mouse_click_maps_to_visible_scrolled_text_column_in_host_editor() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 40);
    app.host_editor = Some(HostEditorState::new_create(
        PathBuf::from("/tmp/inventory.yaml"),
        vec!["default".to_string()],
        vec!["db_prod".to_string()],
    ));

    let (row, text_start_col, expected_cursor) = {
        let (_, inner) = app.host_editor_modal_layout().expect("host editor layout");
        let form = app.host_editor.as_mut().expect("host editor state");
        form.selected = HostEditorField::Host;
        form.host.value = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
        form.host.cursor = form.host.value.chars().count();

        let host_row_index = form
            .visible_fields()
            .iter()
            .position(|field| *field == HostEditorField::Host)
            .expect("host row index") as u16;
        let row = inner.y.saturating_add(2).saturating_add(host_row_index);
        let text_start_col = inner.x.saturating_add("Host: ".chars().count() as u16);
        let value_width = inner.width.saturating_sub("Host: ".chars().count() as u16);
        let expected_cursor = form.field_horizontal_scroll_offset(HostEditorField::Host, value_width);
        (row, text_start_col, expected_cursor)
    };

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: text_start_col,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("mouse click in host text column");

    let form = app.host_editor.as_ref().expect("host editor state");
    assert_eq!(form.selected, HostEditorField::Host);
    assert_eq!(form.host.cursor, expected_cursor);
}
