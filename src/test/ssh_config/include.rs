use super::matches_pattern;

#[test]
fn matches_pattern_star_and_question_wildcards_match_expected_paths() {
    let cases = [
        ("abc.conf", "*.conf", true),
        ("a1.conf", "a?.conf", true),
        ("abc.conf", "a?.conf", false),
        ("abc.txt", "*.conf", false),
    ];

    for (value, pattern, expected) in cases {
        assert_eq!(matches_pattern(value, pattern), expected);
    }
}
