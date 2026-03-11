fn main() -> cossh::Result<std::process::ExitCode> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort terminal restoration so a panic never leaves the
        // terminal stuck in raw mode, alternate screen, or with a hidden
        // cursor. Each call is individually guarded so a failure in one
        // does not prevent the others from running.
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        default_hook(info);
    }));

    cossh::run()
}
