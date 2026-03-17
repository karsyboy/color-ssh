use super::AppState;
use crate::config;
use crate::inventory::{InventoryHost, build_inventory_tree};
use crate::terminal::highlight_overlay::HighlightOverlayEngine;
use crate::terminal::{TerminalChild, TerminalEngine, TerminalGridPoint, TerminalHostCallbacks, TerminalSession};
use crate::test::support::{fs::TestWorkspace, state::TestStateGuard};
use crate::tui::{HostTab, HostTreeRowKind, TerminalSearchState};
use portable_pty::{Child as PtyChild, ChildKiller, ExitStatus};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Debug)]
struct MockTerminalChild {
    killed: Arc<AtomicBool>,
}

#[derive(Debug)]
struct MockTerminalChildKiller {
    killed: Arc<AtomicBool>,
}

impl ChildKiller for MockTerminalChild {
    fn kill(&mut self) -> std::io::Result<()> {
        self.killed.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(MockTerminalChildKiller {
            killed: Arc::clone(&self.killed),
        })
    }
}

impl ChildKiller for MockTerminalChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        self.killed.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self {
            killed: Arc::clone(&self.killed),
        })
    }
}

impl PtyChild for MockTerminalChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        Ok(Some(ExitStatus::with_exit_code(0)))
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        Ok(ExitStatus::with_exit_code(0))
    }

    fn process_id(&self) -> Option<u32> {
        Some(42)
    }
}

fn mock_terminal_session(killed: Arc<AtomicBool>) -> TerminalSession {
    let child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>> = Arc::new(Mutex::new(Box::new(MockTerminalChild { killed })));
    let engine = Arc::new(Mutex::new(TerminalEngine::new_with_host_and_remote_clipboard_policy(
        24,
        80,
        100,
        TerminalHostCallbacks::default(),
        false,
        1024,
    )));
    let exited = Arc::new(Mutex::new(false));
    let render_epoch = Arc::new(AtomicU64::new(0));

    TerminalSession::new(None, None, TerminalChild::Pty(child), engine, exited, render_epoch)
}

fn test_tab(name: &str, session: Option<TerminalSession>) -> HostTab {
    HostTab {
        host: InventoryHost::new(name.to_string()),
        title: name.to_string(),
        session,
        session_error: None,
        highlight_overlay: HighlightOverlayEngine::new(),
        scroll_offset: 0,
        terminal_search: TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    }
}

#[test]
fn handle_terminal_resize_growing_and_shrinking_width_scales_host_panel_proportionally() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_width = 25;

    app.handle_terminal_resize(200, 30);
    assert_eq!(app.host_panel_width, 50);

    app.handle_terminal_resize(120, 30);
    assert_eq!(app.host_panel_width, 30);
}

#[test]
fn handle_terminal_resize_very_small_window_clamps_host_panel_width() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 30);
    app.host_panel_width = 30;

    app.handle_terminal_resize(10, 30);

    assert_eq!(app.host_panel_width, 9);
}

#[test]
fn handle_terminal_resize_window_growth_caps_width_at_default_percent() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_default_percent = 25;
    app.host_panel_width = 60;

    app.handle_terminal_resize(200, 30);

    assert_eq!(app.host_panel_width, 50);
}

#[test]
fn should_draw_when_config_version_changes() {
    let _state = TestStateGuard::lock();
    let mut app = AppState::new_for_tests();
    app.ui_dirty = false;
    app.mark_drawn();

    let original_version = app.last_seen_config_version;
    config::set_config_version(original_version.wrapping_add(1));

    assert!(app.should_draw(std::time::Duration::from_secs(60)));

    config::set_config_version(original_version);
}

#[test]
fn apply_config_reload_notifications_sets_reload_notice_toast() {
    let _state = TestStateGuard::lock();

    let mut app = AppState::new_for_tests();

    config::queue_reload_notice("Config reloaded successfully".to_string());
    app.apply_config_reload_notifications();

    assert_eq!(
        app.reload_notice_toast.as_ref().map(|toast| toast.message()),
        Some("[color-ssh] Config reloaded successfully")
    );
}

#[test]
fn current_selection_orders_typed_terminal_points() {
    let mut app = AppState::new_for_tests();
    app.selection_start = Some(TerminalGridPoint::new(4, 10));
    app.selection_end = Some(TerminalGridPoint::new(2, 3));

    let selection = app.current_selection().expect("current selection");

    assert_eq!(selection.start(), TerminalGridPoint::new(2, 3));
    assert_eq!(selection.end(), TerminalGridPoint::new(4, 10));
}

#[test]
fn terminate_all_sessions_terminates_and_detaches_tab_children() {
    let mut app = AppState::new_for_tests();
    let first_killed = Arc::new(AtomicBool::new(false));
    let second_killed = Arc::new(AtomicBool::new(false));

    app.tabs = vec![
        test_tab("alpha", Some(mock_terminal_session(Arc::clone(&first_killed)))),
        test_tab("beta", Some(mock_terminal_session(Arc::clone(&second_killed)))),
        test_tab("gamma", None),
    ];

    app.terminate_all_sessions();

    assert!(first_killed.load(Ordering::Relaxed));
    assert!(second_killed.load(Ordering::Relaxed));
    assert!(app.tabs.iter().all(|tab| tab.session.is_none()));
}

fn seed_app_from_inventory(app: &mut AppState, inventory_path: &std::path::Path) {
    let tree = build_inventory_tree(inventory_path).expect("load inventory tree");
    app.hosts = tree.hosts;
    app.host_tree_root = tree.root;
    app.host_search_index = AppState::build_host_search_index(&app.hosts);
    app.collapsed_folders.clear();
    app.inventory_load_error = None;
    app.search_query.clear();
    app.update_filtered_hosts();
}

fn find_folder_id(app: &AppState, name: &str) -> usize {
    app.visible_host_rows
        .iter()
        .find_map(|row| match row.kind {
            HostTreeRowKind::Folder(folder_id) if row.display_name == name => Some(folder_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing folder '{name}'"))
}

fn find_host_row(app: &AppState, name: &str) -> usize {
    app.visible_host_rows
        .iter()
        .position(|row| match row.kind {
            HostTreeRowKind::Host(host_idx) => app.hosts.get(host_idx).is_some_and(|host| host.name == name),
            HostTreeRowKind::Folder(_) => false,
        })
        .unwrap_or_else(|| panic!("missing host row '{name}'"))
}

#[test]
fn reload_inventory_tree_from_path_preserves_selection_and_collapsed_folders() {
    let workspace = TestWorkspace::new("tui", "inventory_reload_preserve").expect("temp workspace");
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
  - Other:
      - name: beta
        protocol: ssh
        host: beta.example
"#,
        )
        .expect("write initial inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);

    let other_folder_id = find_folder_id(&app, "Other");
    app.set_folder_expanded(other_folder_id, false);
    let alpha_row = find_host_row(&app, "alpha");
    app.set_selected_row(alpha_row);

    workspace
        .write(
            &inventory_path,
            r#"
inventory:
  - name: rootbox
    protocol: ssh
    host: rootbox.example
  - Group:
      - name: alpha
        protocol: ssh
        host: alpha.example
      - name: gamma
        protocol: ssh
        host: gamma.example
  - Other:
      - name: beta
        protocol: ssh
        host: beta.example
"#,
        )
        .expect("write updated inventory");

    app.reload_inventory_tree_from_path(&inventory_path).expect("reload inventory tree");

    let selected_host_idx = app.selected_host_idx().expect("selected host after reload");
    assert_eq!(app.hosts[selected_host_idx].name, "alpha");
    assert!(app.hosts.iter().any(|host| host.name == "gamma"));
    assert!(app.inventory_load_error.is_none());

    let reloaded_other_folder_id = find_folder_id(&app, "Other");
    assert!(!app.is_folder_expanded(reloaded_other_folder_id));
}

#[test]
fn reload_inventory_tree_from_path_failure_keeps_existing_tree() {
    let workspace = TestWorkspace::new("tui", "inventory_reload_failure").expect("temp workspace");
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
        .expect("write initial inventory");

    let mut app = AppState::new_for_tests();
    seed_app_from_inventory(&mut app, &inventory_path);
    let previous_host_names = app.hosts.iter().map(|host| host.name.clone()).collect::<Vec<_>>();
    let previous_row_count = app.visible_host_rows.len();

    workspace.write(&inventory_path, "inventory: [").expect("write invalid inventory");

    let err = app.reload_inventory_tree_from_path(&inventory_path).expect_err("reload should fail");

    assert!(err.to_string().contains("failed to parse inventory YAML"));
    assert_eq!(app.hosts.iter().map(|host| host.name.clone()).collect::<Vec<_>>(), previous_host_names);
    assert_eq!(app.visible_host_rows.len(), previous_row_count);
    assert!(
        app.inventory_load_error
            .as_deref()
            .is_some_and(|message| message.contains("failed to parse inventory YAML"))
    );
}
