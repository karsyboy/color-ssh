use super::sanitize_session_name;

#[test]
fn sanitizes_session_name_for_log_paths() {
    assert_eq!(sanitize_session_name("prod-host"), "prod-host");
    assert_eq!(sanitize_session_name("my host"), "my_host");
    assert_eq!(sanitize_session_name(".."), "session");
    assert_eq!(sanitize_session_name(""), "session");
}
