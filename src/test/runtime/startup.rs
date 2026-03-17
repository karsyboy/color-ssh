use super::title_banner_viewport_output;

#[test]
fn title_banner_viewport_output_is_absent_when_disabled() {
    assert_eq!(title_banner_viewport_output(false), None);
}

#[test]
fn title_banner_viewport_output_uses_crlf_line_endings() {
    let output = title_banner_viewport_output(true).expect("title banner output should exist");

    assert!(output.contains("Version:"));
    assert!(output.contains("Github:"));
    assert!(output.ends_with("\r\n"));
    assert!(output.matches("\r\n").count() >= 8);
}
