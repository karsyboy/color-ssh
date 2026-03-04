use crate::tui::AppState;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn host_details_shows_inventory_load_error_when_inventory_cannot_be_loaded() {
    let mut app = AppState::new_for_tests();
    app.inventory_load_error = Some("inventory error in 'test': inventory must be a YAML list".to_string());

    let backend = TestBackend::new(64, 7);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| app.render_host_details(frame, frame.area())).expect("draw host details");

    let rendered = buffer_text(terminal.backend());
    assert!(rendered.contains("Host Details"));
    assert!(rendered.contains("Inventory load failed"));
    assert!(rendered.contains("Fix the inventory YAML formatting and included files."));
    assert!(rendered.contains("Reason: inventory error in 'test': inventory must be a YAML list"));
}

fn buffer_text(backend: &TestBackend) -> String {
    let buffer = backend.buffer();
    let mut lines = Vec::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        lines.push(line);
    }
    lines.join("\n")
}
