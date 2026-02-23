use super::SessionManager;
use ratatui::text::Span;

#[test]
fn calculates_span_width_using_unicode_display_width() {
    let spans = vec![Span::raw("a界"), Span::raw("x")];
    assert_eq!(SessionManager::spans_display_width(&spans), 4);
}
