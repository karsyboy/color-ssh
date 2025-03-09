mod errors;
mod macros;
mod prompt;

pub use errors::UIError;
pub use prompt::Prompt;

use crossterm::terminal::ClearType;

/// Defines the different clear options for UI prompts
#[derive(Debug, Clone, Copy)]
pub enum PromptClearMode {
    /// All cells.
    All,
    /// All plus history
    Purge,
    /// All cells from the cursor position downwards.
    FromCursorDown,
    /// All cells from the cursor position upwards.
    FromCursorUp,
    /// All cells at the cursor row.
    CurrentLine,
    /// All cells from the cursor position until the new line.
    UntilNewLine,
}

impl From<PromptClearMode> for ClearType {
    fn from(mode: PromptClearMode) -> Self {
        match mode {
            PromptClearMode::All => ClearType::All,
            PromptClearMode::Purge => ClearType::Purge,
            PromptClearMode::FromCursorDown => ClearType::FromCursorDown,
            PromptClearMode::FromCursorUp => ClearType::FromCursorUp,
            PromptClearMode::CurrentLine => ClearType::CurrentLine,
            PromptClearMode::UntilNewLine => ClearType::UntilNewLine,
        }
    }
}
