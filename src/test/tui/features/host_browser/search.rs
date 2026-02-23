use super::*;

fn search_entry(name: &str, hostname: Option<&str>, user: Option<&str>) -> HostSearchEntry {
    HostSearchEntry {
        name_lower: name.to_string(),
        hostname_lower: hostname.map(str::to_string),
        user_lower: user.map(str::to_string),
    }
}

#[test]
fn strict_matching_is_preferred_over_fuzzy_fallback() {
    let entries = vec![
        search_entry("database", Some("db.internal"), Some("deploy")),
        search_entry("dba-stage", Some("stage.internal"), Some("ops")),
    ];

    let strict_scores = compute_match_scores(&entries, "data");
    assert_eq!(strict_scores.len(), 1);
    assert!(strict_scores.contains_key(&0));

    let fuzzy_scores = compute_match_scores(&entries, "dsg");
    assert_eq!(fuzzy_scores.len(), 1);
    assert!(fuzzy_scores.contains_key(&1));
}

#[test]
fn strict_score_orders_prefix_before_later_matches() {
    let prefix_score = strict_match_score("server-app", "server").unwrap_or_default();
    let later_score = strict_match_score("prod-server", "server").unwrap_or_default();
    assert!(prefix_score > later_score);
}

#[test]
fn no_match_returns_empty_score_map() {
    let entries = vec![search_entry("alpha", Some("alpha.internal"), Some("dev"))];
    let scores = compute_match_scores(&entries, "zzz");
    assert!(scores.is_empty());
}
