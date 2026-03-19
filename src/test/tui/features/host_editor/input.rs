use super::*;
use crate::inventory::build_inventory_tree;
use crate::test::support::fs::TestWorkspace;
use crate::tui::{EditorTabId, EditorTabState, HostEditorField, HostEditorMode, HostEditorSection, HostEditorState, HostEditorVisibleItem, HostTab};
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

fn find_folder_row(app: &AppState, folder_name: &str) -> usize {
    app.visible_host_rows
        .iter()
        .position(|row| matches!(row.kind, crate::tui::HostTreeRowKind::Folder(_)) && row.display_name == folder_name)
        .unwrap_or_else(|| panic!("folder row '{folder_name}' not found"))
}

fn open_test_editor(app: &mut AppState, editor_state: HostEditorState) {
    let editor_id = match editor_state.mode {
        HostEditorMode::Create => EditorTabId::for_new_entry(editor_state.source_file.clone()),
        HostEditorMode::Edit => EditorTabId::ExistingHost {
            source_file: editor_state.source_file.clone(),
            host_name: editor_state.original_name.clone().unwrap_or_else(|| editor_state.name.value.trim().to_string()),
        },
    };

    app.tabs.push(HostTab::new_editor(EditorTabState { id: editor_id, editor_state }));
    app.selected_tab = app.tabs.len().saturating_sub(1);
    app.focus_on_manager = false;
    app.tab_content_area = Rect::new(0, 0, 120, 40);
}

fn editor_body_row_index(form: &HostEditorState, target: HostEditorVisibleItem) -> usize {
    form.visible_items()
        .into_iter()
        .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
        .position(|item| item == target)
        .expect("editor row index")
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

    let form = app.selected_host_editor_mut().expect("host editor state");
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
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Delete.into();
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
fn manager_delete_shortcut_deletes_host_without_opening_editor_tab() {
    let workspace = TestWorkspace::new("tui", "host_browser_delete_shortcut").expect("temp workspace");
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
    app.set_selected_row(find_host_row(&app, "alpha"));

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("open delete confirmation from manager");

    assert_eq!(app.host_delete_confirm.as_ref().map(|confirm| confirm.host_name.as_str()), Some("alpha"));
    assert!(app.selected_host_editor().is_none());

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        .expect("cancel delete from top-level key handler");
    assert!(app.host_delete_confirm.is_none());
    assert!(app.hosts.iter().any(|host| host.name == "alpha"));

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("re-open delete confirmation");
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
        .expect("confirm delete from top-level key handler");

    assert!(app.host_delete_confirm.is_none());
    assert!(app.selected_host_editor().is_none());
    assert!(app.hosts.iter().all(|host| host.name != "alpha"));
    assert!(app.hosts.iter().any(|host| host.name == "beta"));

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(!rendered.contains("name: alpha"));
    assert!(rendered.contains("name: beta"));
}

#[test]
fn manager_delete_shortcut_ignores_folder_rows() {
    let workspace = TestWorkspace::new("tui", "host_browser_delete_folder_guard").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Group:
      - name: alpha
        protocol: ssh
        host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);

    let folder_row_idx = app
        .visible_host_rows
        .iter()
        .position(|row| row.display_name == "Group" && matches!(row.kind, crate::tui::HostTreeRowKind::Folder(_)))
        .expect("folder row");
    app.set_selected_row(folder_row_idx);

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
        .expect("delete shortcut on folder row");

    assert!(app.host_delete_confirm.is_none());
    assert!(app.hosts.iter().any(|host| host.name == "alpha"));
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
        let form = app.selected_host_editor_mut().expect("create host editor state");
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

    let form = app.selected_host_editor_mut().expect("host editor state");
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
        let form = app.selected_host_editor_mut().expect("create host editor state");
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
    assert_eq!(
        menu.actions,
        vec![
            crate::tui::HostContextMenuAction::EditEntry,
            crate::tui::HostContextMenuAction::DuplicateEntry,
            crate::tui::HostContextMenuAction::MoveToFolder,
            crate::tui::HostContextMenuAction::DeleteEntry,
            crate::tui::HostContextMenuAction::Connect,
        ]
    );

    app.host_context_menu = None;

    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 1,
        row: 7,
        modifiers: KeyModifiers::NONE,
    })
    .expect("right click empty host area");

    let menu = app.host_context_menu.as_ref().expect("empty context menu");
    assert_eq!(menu.actions, vec![crate::tui::HostContextMenuAction::NewEntryInFolder]);

    let row_before = app.selected_host_row;
    app.handle_manager_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).expect("manager down");
    assert!(app.selected_host_row >= row_before);
}

#[test]
fn duplicate_entry_opens_prefilled_create_tab_and_saves_without_overwriting_original() {
    let workspace = TestWorkspace::new("tui", "host_editor_duplicate").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Group:
      - name: alpha
        description: Primary host
        protocol: ssh
        host: alpha.example
        user: admin
        port: 2200
        profile: work
        vault-pass: alpha_pass
        identity-file:
          - /tmp/id_alpha
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));
    let host_idx = app.selected_host_idx().expect("selected host idx");

    app.open_host_context_menu_for_selected_host(1, 1, host_idx);
    app.execute_host_context_menu_action(crate::tui::HostContextMenuAction::DuplicateEntry);

    let form = app.selected_host_editor().expect("duplicate host editor");
    assert_eq!(form.mode, HostEditorMode::Create);
    assert!(form.original_name.is_none());
    assert_eq!(form.source_file, inventory_path);
    assert_eq!(form.name.value, "alpha (copy)");
    assert_eq!(form.description.value, "Primary host");
    assert_eq!(form.protocol.value, "ssh");
    assert_eq!(form.host.value, "alpha.example");
    assert_eq!(form.user.value, "admin");
    assert_eq!(form.port.value, "2200");
    assert_eq!(form.profile.value, "work");
    assert_eq!(form.vault_pass.value, "alpha_pass");
    assert!(form.identity_file.value.contains("/tmp/id_alpha"));
    assert_eq!(form.folder_path.value, "/Group/");

    match &app.tabs[app.selected_tab].editor().expect("editor tab").id {
        EditorTabId::DuplicateEntry { source_file, source_host_name } => {
            assert_eq!(source_file, &inventory_path);
            assert_eq!(source_host_name, "alpha");
        }
        other => panic!("expected duplicate editor tab id, got {other:?}"),
    }

    {
        let form = app.selected_host_editor_mut().expect("duplicate host editor");
        form.host.value = "alpha-copy.example".to_string();
    }
    app.submit_host_editor();

    let original = app.hosts.iter().find(|host| host.name == "alpha").expect("original host remains");
    assert_eq!(original.host, "alpha.example");

    let duplicate = app.hosts.iter().find(|host| host.name == "alpha (copy)").expect("duplicated host exists");
    assert_eq!(duplicate.host, "alpha-copy.example");

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("name: alpha"));
    assert!(rendered.contains("name: alpha (copy)"));
    assert!(rendered.contains("alpha-copy.example"));
}

#[test]
fn duplicate_entry_uses_distinct_tab_identity_from_edit_tab() {
    let workspace = TestWorkspace::new("tui", "host_editor_duplicate_tab_identity").expect("temp workspace");
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

    app.open_host_editor_for_selected_host();
    assert_eq!(app.selected_host_editor().expect("edit tab").mode, HostEditorMode::Edit);
    let tabs_before = app.tabs.len();

    let host_idx = app.selected_host_idx().expect("selected host idx");
    app.open_host_context_menu_for_selected_host(1, 1, host_idx);
    app.execute_host_context_menu_action(crate::tui::HostContextMenuAction::DuplicateEntry);

    assert_eq!(app.tabs.len(), tabs_before + 1);
    assert_eq!(app.selected_host_editor().expect("duplicate tab").mode, HostEditorMode::Create);
    assert_eq!(app.selected_host_editor().expect("duplicate tab").name.value, "alpha (copy)");
    assert!(
        app.tabs
            .iter()
            .filter_map(|tab| tab.editor())
            .any(|editor| matches!(editor.id, EditorTabId::ExistingHost { .. })),
        "existing edit tab should remain open"
    );
    assert!(matches!(
        app.tabs[app.selected_tab].editor().expect("selected editor").id,
        EditorTabId::DuplicateEntry { .. }
    ));
}

#[test]
fn protocol_field_cycles_with_arrow_keys_and_filters_visible_fields_by_protocol() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(
            PathBuf::from("/tmp/inventory.yaml"),
            vec!["default".to_string(), "work".to_string()],
            vec!["rdp_lab".to_string()],
        ),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Protocol.into();
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.protocol.value, "rdp");
    assert!(!form.visible_fields().contains(&HostEditorField::Profile));
    assert!(!form.visible_fields().contains(&HostEditorField::IdentityFile));
    assert!(!form.visible_fields().contains(&HostEditorField::RdpDomain));
    assert!(!form.visible_fields().contains(&HostEditorField::RdpArgs));
    assert!(form.visible_sections().contains(&HostEditorSection::Rdp));
    assert!(!form.visible_sections().contains(&HostEditorSection::ProxyForwarding));
    assert!(!form.visible_sections().contains(&HostEditorSection::AdvancedSsh));
    assert!(form.section_collapsed(HostEditorSection::Rdp));

    let form = app.selected_host_editor_mut().expect("host editor state");
    form.selected = HostEditorField::Protocol.into();
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.protocol.value, "ssh");
    assert!(form.visible_fields().contains(&HostEditorField::Profile));
    assert!(form.visible_fields().contains(&HostEditorField::IdentityFile));
    assert!(!form.visible_fields().contains(&HostEditorField::RdpDomain));
    assert!(form.visible_sections().contains(&HostEditorSection::Placement));
    assert!(!form.visible_sections().contains(&HostEditorSection::Rdp));
}

#[test]
fn advanced_ssh_section_defaults_collapsed_and_toggles_with_enter_and_space() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(
            PathBuf::from("/tmp/inventory.yaml"),
            vec!["default".to_string(), "work".to_string()],
            vec!["rdp_lab".to_string()],
        ),
    );

    {
        let form = app.selected_host_editor().expect("host editor state");
        assert!(form.section_collapsed(HostEditorSection::AdvancedSsh));
        assert!(!form.visible_fields().contains(&HostEditorField::SshOptions));
    }

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorVisibleItem::SectionHeader(HostEditorSection::AdvancedSsh);
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    {
        let form = app.selected_host_editor().expect("host editor state");
        assert!(!form.section_collapsed(HostEditorSection::AdvancedSsh));
        assert!(form.visible_fields().contains(&HostEditorField::SshOptions));
    }

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorVisibleItem::SectionHeader(HostEditorSection::AdvancedSsh);
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert!(form.section_collapsed(HostEditorSection::AdvancedSsh));
    assert!(!form.visible_fields().contains(&HostEditorField::SshOptions));
}

#[test]
fn editor_open_defaults_expand_basic_and_authentication_only() {
    let workspace = TestWorkspace::new("tui", "host_editor_section_defaults").expect("temp workspace");
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

    app.open_host_editor_for_new_entry(inventory_path.clone());
    {
        let form = app.selected_host_editor().expect("create host editor state");
        assert!(!form.section_collapsed(HostEditorSection::Basic));
        assert!(!form.section_collapsed(HostEditorSection::Authentication));
        assert!(form.section_collapsed(HostEditorSection::ProxyForwarding));
        assert!(form.section_collapsed(HostEditorSection::AdvancedSsh));
        assert!(form.section_collapsed(HostEditorSection::Rdp));
        assert!(form.section_collapsed(HostEditorSection::Placement));
    }

    app.close_selected_editor_tab();
    app.set_selected_row(find_host_row(&app, "alpha"));
    app.open_host_editor_for_selected_host();
    {
        let form = app.selected_host_editor().expect("edit host editor state");
        assert!(!form.section_collapsed(HostEditorSection::Basic));
        assert!(!form.section_collapsed(HostEditorSection::Authentication));
        assert!(form.section_collapsed(HostEditorSection::ProxyForwarding));
        assert!(form.section_collapsed(HostEditorSection::AdvancedSsh));
        assert!(form.section_collapsed(HostEditorSection::Rdp));
        assert!(form.section_collapsed(HostEditorSection::Placement));
    }
}

#[test]
fn collapsed_sections_are_skipped_by_navigation() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(
            PathBuf::from("/tmp/inventory.yaml"),
            vec!["default".to_string(), "work".to_string()],
            vec!["rdp_lab".to_string()],
        ),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.toggle_section(HostEditorSection::Authentication);
        form.selected = HostEditorVisibleItem::SectionHeader(HostEditorSection::Authentication);
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.selected, HostEditorVisibleItem::SectionHeader(HostEditorSection::ProxyForwarding));
    assert!(!form.visible_fields().contains(&HostEditorField::Profile));
}

#[test]
fn placement_section_is_create_only() {
    let workspace = TestWorkspace::new("tui", "host_editor_placement_section").expect("temp workspace");
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
    app.open_host_editor_for_new_entry(inventory_path.clone());

    {
        let form = app.selected_host_editor().expect("create host editor state");
        assert!(form.visible_sections().contains(&HostEditorSection::Placement));
        assert!(form.section_collapsed(HostEditorSection::Placement));
        assert!(!form.visible_fields().contains(&HostEditorField::FolderPath));
    }

    app.close_selected_editor_tab();
    app.set_selected_row(find_host_row(&app, "alpha"));
    app.open_host_editor_for_selected_host();

    let form = app.selected_host_editor().expect("edit host editor state");
    assert_eq!(form.mode, HostEditorMode::Edit);
    assert!(!form.visible_sections().contains(&HostEditorSection::Placement));
    assert!(!form.visible_fields().contains(&HostEditorField::FolderPath));
}

#[test]
fn profile_field_cycles_with_arrow_keys() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(
            PathBuf::from("/tmp/inventory.yaml"),
            vec!["default".to_string(), "work".to_string()],
            Vec::new(),
        ),
    );

    let form = app.selected_host_editor_mut().expect("host editor state");
    form.selected = HostEditorField::Profile.into();
    assert_eq!(form.mode, HostEditorMode::Create);
    assert_eq!(form.profile.value, "default");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.selected_host_editor().expect("host editor state").profile.value.as_str(), "work");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.selected_host_editor().expect("host editor state").profile.value.as_str(), "default");
}

#[test]
fn vault_pass_field_cycles_with_arrow_keys() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(
            PathBuf::from("/tmp/inventory.yaml"),
            vec!["default".to_string()],
            vec!["db_prod".to_string(), "rdp_lab".to_string()],
        ),
    );

    let form = app.selected_host_editor_mut().expect("host editor state");
    form.selected = HostEditorField::VaultPass.into();
    assert_eq!(form.vault_pass.value, "db_prod");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.selected_host_editor().expect("host editor state").vault_pass.value.as_str(), "rdp_lab");

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.selected_host_editor().expect("host editor state").vault_pass.value.as_str(), "db_prod");
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
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        assert_eq!(form.port.value, "22");
        form.selected = HostEditorField::Protocol.into();
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    {
        let form = app.selected_host_editor().expect("host editor state");
        assert_eq!(form.protocol.value, "rdp");
        assert_eq!(form.port.value, "3389");
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    {
        let form = app.selected_host_editor().expect("host editor state");
        assert_eq!(form.protocol.value, "ssh");
        assert_eq!(form.port.value, "22");
    }

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.port.value = "3390".to_string();
        form.selected = HostEditorField::Protocol.into();
    }
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.protocol.value, "rdp");
    assert_eq!(form.port.value, "3390");
}

#[test]
fn description_field_accepts_space_input() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Description.into();
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.description.value, "a b");
}

#[test]
fn non_description_fields_ignore_space_input() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Host.into();
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.host.value, "ab");
}

#[test]
fn host_editor_paste_preserves_spaces_only_for_description() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Host.into();
    }
    app.handle_host_editor_paste("with spaces");
    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.host.value, "withspaces");

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Description.into();
    }
    app.handle_host_editor_paste("more spaces");
    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.description.value, "more spaces");
}

#[test]
fn identities_only_space_still_cycles_value() {
    let mut app = AppState::new_for_tests();
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::IdentitiesOnly.into();
        assert_eq!(form.identities_only_display(), "auto");
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.identities_only_display(), "yes");
}

#[test]
fn host_editor_mouse_selection_allows_drag_highlight_and_delete() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 40);
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    {
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.host.value = "alpha.example".to_string();
        form.host.cursor = form.host.value.chars().count();
    }

    let (_, inner) = app.host_editor_tab_layout(app.tab_content_area).expect("host editor layout");
    let host_row_index = app
        .selected_host_editor()
        .expect("host editor state")
        .visible_items()
        .into_iter()
        .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
        .position(|item| item == HostEditorVisibleItem::Field(HostEditorField::Host))
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

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.selected, HostEditorVisibleItem::Field(HostEditorField::Host));
    assert_eq!(form.host.value, ".example");
}

#[test]
fn create_mode_folder_path_uses_folder_picker_selection() {
    let workspace = TestWorkspace::new("tui", "host_editor_folder_picker_create").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Group:
      - name: alpha
        protocol: ssh
        host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.open_host_editor_for_new_entry(inventory_path.clone());

    {
        let form = app.selected_host_editor_mut().expect("create editor");
        form.toggle_section(HostEditorSection::Placement);
        form.selected = HostEditorField::FolderPath.into();
    }

    app.handle_host_editor_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.folder_picker.is_some(), "folder picker should open from placement field");

    {
        let picker = app.folder_picker.as_mut().expect("folder picker");
        picker.selected = picker
            .rows
            .iter()
            .position(|row| row.folder_path == vec!["Group".to_string()])
            .expect("group folder option");
    }
    app.handle_folder_picker_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let form = app.selected_host_editor().expect("create editor");
    assert_eq!(form.folder_path.value, "/Group/");
}

#[test]
fn move_to_folder_context_action_moves_host_and_refreshes_browser() {
    let workspace = TestWorkspace::new("tui", "host_editor_move_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - TeamA:
      - name: alpha
        protocol: ssh
        host: alpha.example
  - TeamB:
      - name: beta
        protocol: ssh
        host: beta.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    app.set_selected_row(find_host_row(&app, "alpha"));
    let host_idx = app.selected_host_idx().expect("selected host");

    app.open_host_context_menu_for_selected_host(1, 1, host_idx);
    app.execute_host_context_menu_action(crate::tui::HostContextMenuAction::MoveToFolder);
    assert!(app.folder_picker.is_some(), "move action should open folder picker");

    {
        let picker = app.folder_picker.as_mut().expect("folder picker");
        picker.selected = picker
            .rows
            .iter()
            .position(|row| row.folder_path == vec!["TeamB".to_string()])
            .expect("team b folder option");
    }
    app.handle_folder_picker_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let moved = app.hosts.iter().find(|host| host.name == "alpha").expect("moved host");
    assert_eq!(moved.source_folder_path, vec!["TeamB".to_string()]);

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("TeamB:"));
    assert!(rendered.contains("name: alpha"));
}

#[test]
fn rename_folder_context_action_renames_folder_and_updates_tree() {
    let workspace = TestWorkspace::new("tui", "host_editor_rename_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Old:
      - name: alpha
        protocol: ssh
        host: alpha.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);

    let folder_row = find_folder_row(&app, "Old");
    let folder_id = match app.visible_host_rows[folder_row].kind {
        crate::tui::HostTreeRowKind::Folder(folder_id) => folder_id,
        _ => panic!("expected folder row"),
    };

    app.open_host_context_menu_for_folder(1, 1, folder_id, inventory_path.clone());
    app.execute_host_context_menu_action(crate::tui::HostContextMenuAction::RenameFolder);
    assert!(app.folder_rename.is_some(), "rename action should open folder rename modal");

    {
        let state = app.folder_rename.as_mut().expect("rename modal");
        state.name = "New".to_string();
        state.cursor = state.name.chars().count();
        state.selection = None;
    }
    app.handle_folder_rename_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.visible_host_rows.iter().any(|row| row.display_name == "New"));
    assert!(!app.visible_host_rows.iter().any(|row| row.display_name == "Old"));

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("New:"));
    assert!(!rendered.contains("Old:"));
}

#[test]
fn delete_folder_context_action_opens_confirm_with_entry_count_and_deletes() {
    let workspace = TestWorkspace::new("tui", "host_editor_delete_folder").expect("temp workspace");
    let inventory_path = workspace.join("cossh-inventory.yaml");
    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - Keep:
      - name: keep-host
        protocol: ssh
        host: keep.example
  - Remove:
      - name: alpha
        protocol: ssh
        host: alpha.example
      - Nested:
          - name: beta
            protocol: ssh
            host: beta.example
"#,
        )
        .expect("write inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);

    let folder_row = find_folder_row(&app, "Remove");
    let folder_id = match app.visible_host_rows[folder_row].kind {
        crate::tui::HostTreeRowKind::Folder(folder_id) => folder_id,
        _ => panic!("expected folder row"),
    };

    app.open_host_context_menu_for_folder(1, 1, folder_id, inventory_path.clone());
    app.execute_host_context_menu_action(crate::tui::HostContextMenuAction::DeleteFolder);

    let confirm = app.folder_delete_confirm.as_ref().expect("folder delete confirmation");
    assert_eq!(confirm.folder_name, "Remove");
    assert_eq!(confirm.removed_entry_count, 2);

    app.handle_folder_delete_confirm_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.hosts.iter().any(|host| host.name == "keep-host"));
    assert!(app.hosts.iter().all(|host| host.name != "alpha"));
    assert!(app.hosts.iter().all(|host| host.name != "beta"));

    let rendered = fs::read_to_string(&inventory_path).expect("read inventory");
    assert!(rendered.contains("Keep:"));
    assert!(!rendered.contains("Remove:"));
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
    app.tab_content_area = Rect::new(0, 0, 120, 40);

    let (_, inner) = app.host_editor_tab_layout(app.tab_content_area).expect("host editor layout");
    let action_row = {
        let form = app.selected_host_editor().expect("host editor state");
        let non_action_rows = form
            .visible_items()
            .iter()
            .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
            .count() as u16;
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
    assert!(app.selected_host_editor().is_none());
}

#[test]
fn host_editor_modal_height_includes_bottom_action_row() {
    let form = HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]);
    let non_action_rows = form
        .visible_items()
        .iter()
        .filter(|item| !matches!(item, HostEditorVisibleItem::Field(field) if field.is_action()))
        .count() as u16;
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
    form.selected = HostEditorField::Host.into();
    form.host.value = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
    form.host.cursor = form.host.value.chars().count();

    let scroll = form.field_horizontal_scroll_offset(HostEditorField::Host, 16);
    assert!(scroll > 0, "expected non-zero horizontal scroll for long value at end cursor");
}

#[test]
fn mouse_click_maps_to_visible_scrolled_text_column_in_host_editor() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 40);
    open_test_editor(
        &mut app,
        HostEditorState::new_create(PathBuf::from("/tmp/inventory.yaml"), vec!["default".to_string()], vec!["db_prod".to_string()]),
    );

    let (row, text_start_col, expected_cursor) = {
        let (_, inner) = app.host_editor_tab_layout(app.tab_content_area).expect("host editor layout");
        let form = app.selected_host_editor_mut().expect("host editor state");
        form.selected = HostEditorField::Host.into();
        form.host.value = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
        form.host.cursor = form.host.value.chars().count();

        let host_row_index = editor_body_row_index(form, HostEditorVisibleItem::Field(HostEditorField::Host)) as u16;
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

    let form = app.selected_host_editor().expect("host editor state");
    assert_eq!(form.selected, HostEditorVisibleItem::Field(HostEditorField::Host));
    assert_eq!(form.host.cursor, expected_cursor);
}
