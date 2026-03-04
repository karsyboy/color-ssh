use super::{AnsiColorState, CompiledHighlightRule, HighlightScratch, process_chunk_with_scratch};
use regex::{Regex, RegexSet};

fn compiled_rule(pattern: &str, style: &str) -> CompiledHighlightRule {
    CompiledHighlightRule::new(Regex::new(pattern).expect("regex"), style.to_string())
}

fn process_chunk_once(chunk: &str, chunk_id: i32, rules: &[CompiledHighlightRule], rule_set: Option<&RegexSet>, reset_color: &str) -> String {
    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();

    process_chunk_with_scratch(chunk, chunk_id, rules, rule_set, reset_color, &mut color_state, &mut scratch).into_owned()
}

#[test]
fn highlight_core_match_and_overlap_behavior() {
    let output = process_chunk_once("\x1b[31merror\x1b[0m", 0, &[compiled_rule("error", "<red>")], None, "</red>");
    assert!(output.contains("<red>error"));

    let overlap = process_chunk_once("abc", 1, &[compiled_rule("ab", "<a>"), compiled_rule("abc", "<b>")], None, "</>");
    assert_eq!(overlap, "<a>ab</>c");
}

#[test]
fn highlight_prefilter_matches_full_scan() {
    let rules = vec![compiled_rule("error", "<r>"), compiled_rule("warn", "<y>"), compiled_rule("ok", "<g>")];
    let patterns: Vec<&str> = rules.iter().map(|rule| rule.regex.as_str()).collect();
    let rule_set = RegexSet::new(patterns).expect("regex set");

    let with_prefilter = process_chunk_once("warn and error", 2, &rules, Some(&rule_set), "</>");
    let without_prefilter = process_chunk_once("warn and error", 2, &rules, None, "</>");
    assert_eq!(with_prefilter, without_prefilter);
}

#[test]
fn ansi_color_restore_core_behavior() {
    let rules = vec![compiled_rule("down", "\x1b[31m")];
    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();

    process_chunk_with_scratch("\x1b[32m", 3, &rules, None, "\x1b[0m", &mut color_state, &mut scratch);
    let highlighted = process_chunk_with_scratch("down", 4, &rules, None, "\x1b[0m", &mut color_state, &mut scratch).into_owned();
    assert_eq!(highlighted, "\x1b[31mdown\x1b[32m");
}
