use super::{
    ansi_black, ansi_bright_black, ansi_bright_cyan, ansi_bright_red, ansi_bright_white, ansi_cyan, ansi_green, ansi_magenta, ansi_red, ansi_white,
    ansi_yellow, display_width, selection_bg, selection_fg, tab_active_bg, tab_inactive_bg, truncate_to_display_width,
};
use ratatui::style::Color;

#[test]
fn exposes_terminal_ansi_index_colors() {
    assert_eq!(ansi_black(), Color::Indexed(0));
    assert_eq!(ansi_red(), Color::Indexed(1));
    assert_eq!(ansi_green(), Color::Indexed(2));
    assert_eq!(ansi_yellow(), Color::Indexed(3));
    assert_eq!(ansi_magenta(), Color::Indexed(5));
    assert_eq!(ansi_cyan(), Color::Indexed(6));
    assert_eq!(ansi_white(), Color::Indexed(7));
    assert_eq!(ansi_bright_black(), Color::Indexed(8));
    assert_eq!(ansi_bright_red(), Color::Indexed(9));
    assert_eq!(ansi_bright_cyan(), Color::Indexed(14));
    assert_eq!(ansi_bright_white(), Color::Indexed(15));
}

#[test]
fn uses_terminal_defaults_for_tab_and_selection_surfaces() {
    assert_eq!(tab_active_bg(), Color::Indexed(8));
    assert_eq!(tab_inactive_bg(), Color::Reset);
    assert_eq!(selection_fg(), Color::Indexed(0));
    assert_eq!(selection_bg(), Color::Indexed(6));
}

#[test]
fn calculates_display_width_for_wide_glyphs() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("a界b"), 4);
}

#[test]
fn truncates_by_display_width_instead_of_char_count() {
    assert_eq!(truncate_to_display_width("a界b", 1), "a");
    assert_eq!(truncate_to_display_width("a界b", 3), "a界");
    assert_eq!(truncate_to_display_width("a界b", 4), "a界b");
}
