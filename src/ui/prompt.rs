use super::{PromptClearMode, UIError};
use crossterm::{
    cursor,
    event::{self, KeyCode, KeyEvent, KeyModifiers},
    execute, queue, terminal,
};
use regex::Regex;
use rpassword::prompt_password;
use secrecy::{ExposeSecret, SecretBox};
use std::io::{Write, stdin, stdout};

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub struct Prompt {
    cursor_pos: Option<(u16, u16)>,
    clear_mode: PromptClearMode,
    /// Provide a ANSI escape code to set the text color of the selected item.
    select_color: String,
    /// Provide a ANSI escape code to set the text color of the unselected item.
    unselect_color: String,
    /// UTF-8 character to use as a marker for the selected item.
    marker: char,
    help_msg: bool,
}

impl Prompt {
    pub fn default() -> Self {
        Self {
            cursor_pos: None,
            clear_mode: PromptClearMode::UntilNewLine,
            select_color: String::from("\x1B[1;31m"),
            unselect_color: String::from("\x1B[1;34m"),
            marker: '▶',
            help_msg: true,
        }
    }

    pub fn create(
        cursor_pos: Option<(u16, u16)>,
        clear_mode: PromptClearMode,
        select_color: String,
        unselect_color: String,
        marker: char,
        help_msg: bool,
    ) -> Self {
        Self {
            cursor_pos,
            clear_mode,
            select_color,
            unselect_color,
            marker,
            help_msg,
        }
    }

    pub fn set_clear_mode(&mut self, clear_mode: PromptClearMode) {
        self.clear_mode = clear_mode;
    }

    pub fn set_cursor_pos(&mut self, cursor_pos: Option<(u16, u16)>) {
        self.cursor_pos = cursor_pos;
    }

    pub fn set_select_color(&mut self, select_color: String) {
        self.select_color = select_color;
    }

    pub fn set_unselect_color(&mut self, unselect_color: String) {
        self.unselect_color = unselect_color;
    }

    pub fn set_marker(&mut self, marker: char) {
        self.marker = marker;
    }

    pub fn set_help_msg(&mut self, help_msg: bool) {
        self.help_msg = help_msg;
    }

    pub fn selectable_prompt(&self, question: &str, options: &[&str], search_enabled: bool) -> Option<String> {
        let _guard = TerminalGuard;

        let mut filtered_options = options.to_vec();
        let mut selected = 0;
        let mut input = String::new();

        terminal::enable_raw_mode().expect("Failed to enable raw mode");

        let mut stdout = stdout();

        loop {
            queue!(stdout, cursor::MoveTo(0, 0), terminal::Clear(PromptClearMode::All.into())).unwrap();
            if self.help_msg {
                write!(stdout, "{} Use ↑ ↓ to navigate  |  Enter to select  |  ESC to quit", question).unwrap();
            } else {
                write!(stdout, "{}", question).unwrap();
            }

            if search_enabled {
                write!(stdout, "\r\nSearch: {}\r\n", input).unwrap();
            } else {
                write!(stdout, "\r\n").unwrap();
            }

            for (i, option) in filtered_options.iter().enumerate() {
                let marker = if i == selected { self.marker } else { ' ' };
                let color = if i == selected {
                    self.select_color.clone()
                } else {
                    self.unselect_color.clone()
                };
                write!(stdout, "\r\n{} {} {} \x1B[0m", color, marker, option).unwrap();
            }

            stdout.flush().unwrap();

            if let event::Event::Key(KeyEvent { code, .. }) = event::read().unwrap() {
                match code {
                    KeyCode::Up if selected > 0 => selected -= 1,
                    KeyCode::Down if selected < filtered_options.len().saturating_sub(1) => selected += 1,
                    KeyCode::Enter => {
                        return filtered_options.get(selected).cloned().map(String::from);
                    }
                    KeyCode::Char(c) if search_enabled => {
                        input.push(c);
                        filtered_options = fuzzy_search(options, &input);
                        selected = 0;
                    }
                    KeyCode::Backspace if search_enabled => {
                        input.pop();
                        filtered_options = fuzzy_search(options, &input);
                        selected = 0;
                    }
                    KeyCode::Esc => return None,
                    _ => {}
                }
            }
        }
    }

    pub fn yes_no_prompt(&self, question: &str, default: bool) -> bool {
        self.bool_prompt(question, ["Yes", "No"], if default { 0 } else { 1 })
    }

    pub fn true_false_prompt(&self, question: &str, default: bool) -> bool {
        self.bool_prompt(question, ["True", "False"], if default { 0 } else { 1 })
    }

    fn bool_prompt(&self, question: &str, options: [&str; 2], default: usize) -> bool {
        let _guard = TerminalGuard;
        let mut selected = default;
        let mut stdout = stdout();

        terminal::enable_raw_mode().expect("Failed to enable raw mode");

        // Print the initial prompt
        if let Some((x, y)) = self.cursor_pos {
            queue!(stdout, cursor::MoveTo(x, y), terminal::Clear(self.clear_mode.into())).unwrap();
        } else {
            queue!(stdout, terminal::Clear(self.clear_mode.into())).unwrap();
        }

        if self.help_msg {
            write!(stdout, "\r\n{} (Use ←/→ or ↑/↓ to navigate, Enter to select): ", question).unwrap();
        } else {
            write!(stdout, "\r\n{}: ", question).unwrap();
        }

        stdout.flush().unwrap();

        loop {
            queue!(
                stdout,
                if self.help_msg {
                    cursor::MoveToColumn(question.len() as u16 + 47)
                } else {
                    cursor::MoveToColumn(question.len() as u16 + 2)
                }
            )
            .unwrap();

            for (i, option) in options.iter().enumerate() {
                let marker = if i == selected { self.marker } else { ' ' };
                let color = if i == selected {
                    self.select_color.clone()
                } else {
                    self.unselect_color.clone()
                };
                write!(stdout, "{} {} {} \x1B[0m", color, marker, option).unwrap();
            }

            stdout.flush().unwrap();

            if let event::Event::Key(KeyEvent { code, modifiers, .. }) = event::read().unwrap() {
                match code {
                    KeyCode::Left | KeyCode::Up => selected = (selected + 1) % 2, // Toggle selection
                    KeyCode::Right | KeyCode::Down => selected = (selected + 1) % 2,
                    KeyCode::Enter => return selected == 0, // Select first option as true
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        drop(_guard);
                        std::process::exit(0)
                    }
                    KeyCode::Char(c) => match c.to_ascii_lowercase() {
                        'y' if options == ["Yes", "No"] => return true,
                        'n' if options == ["Yes", "No"] => return false,
                        't' if options == ["True", "False"] => return true,
                        'f' if options == ["True", "False"] => return false,
                        _ => {}
                    },
                    KeyCode::Esc => return default == 0, // Esc defaults to initial option
                    _ => {}
                }
            }
        }
    }

    pub fn validated_input_prompt(&self, question: &str, pattern: &str, error_message: &str) -> String {
        let regex = Regex::new(pattern).expect("Invalid regex pattern");
        let mut input = String::new();
        let mut stdout = stdout();

        loop {
            // Position cursor if specified
            if let Some((x, y)) = self.cursor_pos {
                queue!(stdout, cursor::MoveTo(x, y), terminal::Clear(self.clear_mode.into())).unwrap();
            } else {
                queue!(stdout, terminal::Clear(self.clear_mode.into())).unwrap();
            }
            print!("{}: ", question);
            stdout.flush().unwrap();
            input.clear();
            stdin().read_line(&mut input).unwrap();
            let input = input.trim().to_string();

            if regex.is_match(&input) {
                return input;
            } else {
                println!("{}", error_message);
            }
        }
    }

    pub fn password_prompt(&self) -> Result<String, UIError> {
        let mut stdout = stdout();
        let mut move_up = 1;
        println!("");
        loop {
            // Position cursor if specified
            if let Some((x, y)) = self.cursor_pos {
                queue!(stdout, cursor::MoveTo(x, y), terminal::Clear(self.clear_mode.into())).unwrap();
            } else {
                queue!(stdout, terminal::Clear(self.clear_mode.into())).unwrap();
            }
            // Prompt for passwords
            let password = SecretBox::new(Box::new(prompt_password("Enter your password: ")?));
            if password.expose_secret().is_empty() {
                execute!(stdout, cursor::MoveUp(move_up), terminal::Clear(PromptClearMode::FromCursorDown.into())).unwrap();
                if move_up == 1 {
                    move_up += 1;
                }
                println!("Password cannot be empty. Please try again.");
                continue;
            }

            let verified_password = SecretBox::new(Box::new(prompt_password("Verify your password: ")?));
            if password.expose_secret() != verified_password.expose_secret() {
                execute!(stdout, cursor::MoveUp(2), terminal::Clear(PromptClearMode::FromCursorDown.into())).unwrap();
                if move_up == 1 {
                    move_up += 1;
                }
                println!("Passwords do not match. Please try again.");
                continue;
            }

            return Ok(password.expose_secret().to_string());
        }
    }
}

/// Performs a fuzzy search on a list of options based on a query.
fn fuzzy_search<'a>(options: &'a [&'a str], query: &str) -> Vec<&'a str> {
    let query = query.to_lowercase();
    options.iter().filter(|&&option| option.to_lowercase().contains(&query)).copied().collect()
}
