use super::matches_pattern;

#[test]
fn matches_pattern_supports_star_and_question() {
    assert!(matches_pattern("abc.conf", "*.conf"));
    assert!(matches_pattern("a1.conf", "a?.conf"));
    assert!(!matches_pattern("abc.conf", "a?.conf"));
    assert!(!matches_pattern("abc.txt", "*.conf"));
}
