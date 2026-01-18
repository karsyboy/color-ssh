// use super::KeepassVault;
// use crate::{log_debug, vault::keepass::EntryPath};
// //     db::{Database, NodeRef},
// //     error::{DatabaseKeyError, DatabaseOpenError},
// //     DatabaseKey,
// // };

// /// Processes the "show" subcommand.
// /// It retrieves the vault entry name from the command-line arguments and prints it.
// pub fn run(keepass_vault: &KeepassVault) {
//     // log_debug!("Vault file: {:?}", keepass_vault);
//     // println!("🔐 Vault file: {:#?}", keepass_vault);

//     let entries = keepass_vault.get_all_entries();
//     for entry in entries {
//         let e = entry.get_entry();
//         println!("Title: {:?}", e.get_title());
//         println!("Path: {:?}", entry.entry_path());
//         println!("Username: {:?}", e.get_username());
//         println!("Password: {:?}", e.get_password());
//     }
// }
use super::KeepassVault;
use crate::{log_debug, vault::keepass::{EntryPath, WrappedEntry}};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Terminal,
};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use crossterm::cursor::{self, MoveTo};
use std::io::{self, stdout, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub fn run(keepass_vault: &KeepassVault) -> Result<Option<WrappedEntry<'_>>, Box<dyn std::error::Error>> {
    let mut stdout = stdout();
    writeln!(stdout, "Some test output before the UI")?;
    writeln!(stdout, "Another line of test output")?;
    stdout.flush()?;

    let (_, mut cursor_y) = cursor::position()?;
    cursor_y += 1;
    execute!(stdout, MoveTo(0, cursor_y))?;

    enable_raw_mode()?;
    execute!(stdout, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let entries = keepass_vault.get_all_entries();
    let total_entries = entries.len();
    let (_, terminal_height) = terminal_size()?;

    let table_height = 8;
    let table_y = if cursor_y + table_height < terminal_height {
        cursor_y
    } else {
        cursor_y.saturating_sub(table_height)
    };

    let mut scroll_offset = 0;
    let mut selected_index = 0;
    let visible_entries = table_height as usize;

    let mut last_render_time = Instant::now();

    loop {
        let now = Instant::now();
        if now.duration_since(last_render_time) >= Duration::from_millis(16) { // ~60 FPS
            terminal.draw(|f| {
                let size = f.area();
                let popup_area = Rect {
                    x: size.x,
                    y: table_y,
                    width: size.width,
                    height: table_height,
                };

                let widths = [
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                ];

                let rows: Vec<Row> = entries.iter()
                    .skip(scroll_offset)
                    .take(visible_entries)
                    .enumerate()
                    .map(|(i, entry)| {
                        let e = entry.get_entry();
                        let style = if i + scroll_offset == selected_index {
                            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        Row::new(vec![
                            Cell::from(e.get_title().unwrap_or_default()).style(style),
                            Cell::from(entry.entry_path()).style(style),
                            Cell::from(e.get_username().unwrap_or_default()).style(style),
                        ])
                    }).collect();

                let table = Table::new(rows, widths)
                    .header(
                        Row::new(vec!["Title", "Path", "Username"])
                            .style(Style::default().fg(Color::Yellow)),
                    )
                    .block(Block::default().borders(Borders::ALL).title("Vault Entries"));

                f.render_widget(table, popup_area);
            })?;
            last_render_time = now;
        }

        if event::poll(Duration::from_millis(5))? {  // Reduced polling delay
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Down => {
                        if selected_index < total_entries - 1 {
                            selected_index += 1;
                            if selected_index >= scroll_offset + visible_entries {
                                scroll_offset += 1;
                            }
                        }
                    }
                    KeyCode::Up => {
                        if selected_index > 0 {
                            selected_index -= 1;
                            if selected_index < scroll_offset {
                                scroll_offset -= 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), DisableMouseCapture)?;
                        terminal.show_cursor()?;
                        println!("Selected entry: {:?}", entries[selected_index]);
                        return Ok(Some(entries[selected_index].clone()));
                    }
                    KeyCode::Char('q') => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(None)
}


