use super::sanitize_session_name;

#[test]
fn sanitize_session_name_unsafe_or_empty_names_map_to_safe_path_segment() {
    let cases = [("prod-host", "prod-host"), ("my host", "my_host"), ("..", "session"), ("", "session")];

    for (input, expected) in cases {
        assert_eq!(sanitize_session_name(input), expected);
    }
}
