use super::CompiledHighlightRule;
use super::ansi::{AnsiColorState, RuleResetMode, advance_color_state_to, push_color_restore_sequence};

pub(super) fn render_highlighted_chunk(
    chunk: &str,
    rules: &[CompiledHighlightRule],
    matches: &[(usize, usize, usize)],
    reset_color: &str,
    should_scan_color_state: bool,
    color_state: &mut AnsiColorState,
    highlighted: &mut String,
) -> usize {
    let mut running_color_state = color_state.clone();
    let mut scan_index = 0usize;
    let mut last_index = 0usize;
    let mut accepted_match_count = 0usize;

    for (start, end, rule_idx) in matches.iter().copied() {
        if last_index > start {
            continue;
        }
        accepted_match_count = accepted_match_count.saturating_add(1);

        highlighted.push_str(&chunk[last_index..start]);
        highlighted.push_str(&rules[rule_idx].style);
        highlighted.push_str(&chunk[start..end]);
        if should_scan_color_state {
            advance_color_state_to(chunk, &mut scan_index, end, &mut running_color_state);
        }

        match rules[rule_idx].reset_mode {
            RuleResetMode::Dynamic { restore_fg, restore_bg } => {
                push_color_restore_sequence(highlighted, &running_color_state, restore_fg, restore_bg);
            }
            RuleResetMode::Static => highlighted.push_str(reset_color),
        }
        last_index = end;
    }

    highlighted.push_str(&chunk[last_index..]);
    if should_scan_color_state {
        advance_color_state_to(chunk, &mut scan_index, chunk.len(), &mut running_color_state);
        *color_state = running_color_state;
    }

    accepted_match_count
}
