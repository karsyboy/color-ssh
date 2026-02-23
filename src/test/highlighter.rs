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
fn highlights_text_when_match_exists_inside_ansi_sequences() {
    let rules = vec![compiled_rule("error", "<red>")];
    let chunk = "\x1b[31merror\x1b[0m".to_string();

    let output = process_chunk_once(&chunk, 0, &rules, None, "</red>");
    assert!(output.contains("<red>error"));
    assert!(output.ends_with("</red>"));
}

#[test]
fn keeps_first_match_when_ranges_overlap() {
    let rules = vec![compiled_rule("ab", "<a>"), compiled_rule("abc", "<b>")];

    let output = process_chunk_once("abc", 1, &rules, None, "</>");
    assert_eq!(output, "<a>ab</>c");
}

#[test]
fn keeps_rule_order_for_equal_start_overlaps() {
    let rules = vec![compiled_rule("abc", "<first>"), compiled_rule("ab", "<second>")];

    let output = process_chunk_once("abc", 7, &rules, None, "</>");
    assert_eq!(output, "<first>abc</>");
}

#[test]
fn maps_newlines_as_spaces_for_matching_but_preserves_raw_text() {
    let rules = vec![compiled_rule("a b", "<x>")];

    let output = process_chunk_once("a\nb", 2, &rules, None, "</x>");
    assert_eq!(output, "<x>a\nb</x>");
}

#[test]
fn returns_original_chunk_when_no_rules_exist() {
    let output = process_chunk_once("plain text", 3, &[], None, "</>");
    assert_eq!(output, "plain text");
}

#[test]
fn prefilter_rule_set_matches_same_output_as_full_scan() {
    let rules = vec![compiled_rule("error", "<r>"), compiled_rule("warn", "<y>"), compiled_rule("ok", "<g>")];
    let patterns: Vec<&str> = rules.iter().map(|rule| rule.regex.as_str()).collect();
    let rule_set = RegexSet::new(patterns).expect("regex set");
    let chunk = "warn and error and ok";

    let with_prefilter = process_chunk_once(chunk, 4, &rules, Some(&rule_set), "</>");
    let without_prefilter = process_chunk_once(chunk, 4, &rules, None, "</>");
    assert_eq!(with_prefilter, without_prefilter);
}

#[test]
fn scratch_path_matches_single_shot_output_for_plain_text() {
    let rules = vec![compiled_rule("status", "<c>")];
    let chunk = "status ok".to_string();

    let single_shot = process_chunk_once(&chunk, 5, &rules, None, "</c>");

    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();
    let from_scratch = process_chunk_with_scratch(&chunk, 5, &rules, None, "</c>", &mut color_state, &mut scratch).into_owned();

    assert_eq!(single_shot, from_scratch);
}

#[test]
fn scratch_path_matches_single_shot_output_for_ansi_text() {
    let rules = vec![compiled_rule("error", "<e>")];
    let chunk = "\x1b[31merror\x1b[0m happened".to_string();

    let single_shot = process_chunk_once(&chunk, 6, &rules, None, "</e>");

    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();
    let from_scratch = process_chunk_with_scratch(&chunk, 6, &rules, None, "</e>", &mut color_state, &mut scratch).into_owned();

    assert_eq!(single_shot, from_scratch);
}

#[test]
fn ansi_highlight_avoids_hard_reset_and_uses_color_restore() {
    let rules = vec![compiled_rule("down", "\x1b[31m")];
    let chunk = "\x1b[7mdown\x1b[27m";

    let output = process_chunk_once(chunk, 8, &rules, None, "\x1b[0m");

    assert!(output.contains("\x1b[31mdown"));
    assert!(output.contains("\x1b[39m"));
    assert!(!output.contains("\x1b[0m"));
}

#[test]
fn restores_previous_foreground_color_across_chunks() {
    let rules = vec![compiled_rule("down", "\x1b[31m")];
    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();

    let no_match = process_chunk_with_scratch("\x1b[32m", 9, &rules, None, "\x1b[0m", &mut color_state, &mut scratch);
    assert_eq!(no_match, "\x1b[32m");

    let highlighted = process_chunk_with_scratch("down", 10, &rules, None, "\x1b[0m", &mut color_state, &mut scratch);
    assert_eq!(highlighted, "\x1b[31mdown\x1b[32m");
}

#[test]
fn no_rules_path_still_tracks_active_colors_for_later_highlights() {
    let rules = vec![compiled_rule("up", "\x1b[31m")];
    let mut scratch = HighlightScratch::default();
    let mut color_state = AnsiColorState::default();

    let passthrough = process_chunk_with_scratch("\x1b[34m", 11, &[], None, "\x1b[0m", &mut color_state, &mut scratch);
    assert_eq!(passthrough, "\x1b[34m");

    let highlighted = process_chunk_with_scratch("up", 12, &rules, None, "\x1b[0m", &mut color_state, &mut scratch);
    assert_eq!(highlighted, "\x1b[31mup\x1b[34m");
}
