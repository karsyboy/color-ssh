use crate::{debug_enabled, log_debug};
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use std::{borrow::Cow, fmt::Write as _, thread, time::Instant};

// Compiled regex for stripping ANSI escape sequences before pattern matching.
static ANSI_ESCAPE_REGEX: Lazy<Option<Regex>> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]    # CSI: ESC [ params intermediates final
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)           # OSC: ESC ] ... (BEL or ESC \)
        |\x1B[PX^_].*?\x1B\\                         # DCS/SOS/PM/APC: ESC P/X/^/_ ... ESC \
        |\x1B.                                       # Other ESC sequences (2 bytes)
        |\x1B                                        # Stray ESC character
    ",
    )
    .ok()
});

// Heuristic: regex-set prefiltering tends to help with smaller rule sets, but
// can become net overhead once the rule list grows.
const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;

#[derive(Default)]
pub struct HighlightScratch {
    clean_chunk: String,
    mapping: Vec<usize>,
    matches: Vec<(usize, usize, usize)>,
    highlighted: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsiColor {
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum AnsiParserState {
    #[default]
    Ground,
    Esc,
    Csi {
        raw: [u8; MAX_SGR_RAW_BYTES],
        len: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiColorState {
    fg: Option<AnsiColor>,
    bg: Option<AnsiColor>,
    parser_state: AnsiParserState,
}

impl Default for AnsiColorState {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            parser_state: AnsiParserState::Ground,
        }
    }
}

impl AnsiColorState {
    fn should_scan(&self, has_ansi: bool) -> bool {
        has_ansi || !matches!(self.parser_state, AnsiParserState::Ground)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleResetMode {
    Dynamic { restore_fg: bool, restore_bg: bool },
    Static,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledHighlightRule {
    pub(crate) regex: Regex,
    pub(crate) style: String,
    pub(crate) reset_mode: RuleResetMode,
}

impl CompiledHighlightRule {
    pub(crate) fn new(regex: Regex, style: String) -> Self {
        let reset_mode = analyze_rule_reset_mode(&style);
        Self { regex, style, reset_mode }
    }
}

/// Processes a chunk using reusable scratch buffers to reduce per-chunk allocations.
pub(crate) fn process_chunk_with_scratch<'a>(
    chunk: &'a str,
    chunk_id: i32,
    rules: &[CompiledHighlightRule],
    rule_set: Option<&RegexSet>,
    reset_color: &str,
    color_state: &mut AnsiColorState,
    scratch: &'a mut HighlightScratch,
) -> Cow<'a, str> {
    // Fast path: nothing to do, so return original chunk without allocating.
    if chunk.is_empty() {
        return Cow::Borrowed(chunk);
    }
    let has_ansi = chunk.as_bytes().contains(&0x1b);
    let should_scan_color_state = color_state.should_scan(has_ansi);
    if rules.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let debug_logging = debug_enabled!();
    let thread_id = debug_logging.then(|| thread::current().id());

    // Timers are created only in debug mode so production hot path stays lean.
    let build_started_at = debug_logging.then(Instant::now);
    let has_newline_or_cr = chunk.as_bytes().iter().any(|byte| matches!(*byte, b'\n' | b'\r'));

    let (clean_chunk, use_mapping) = if has_ansi {
        // ANSI path: strip escape bytes for matching while tracking clean->raw offsets.
        build_index_mapping(chunk, &mut scratch.clean_chunk, &mut scratch.mapping);
        (scratch.clean_chunk.as_str(), true)
    } else if has_newline_or_cr {
        // Newline path: map line breaks to spaces so cross-line patterns can match.
        build_clean_chunk_no_ansi(chunk, &mut scratch.clean_chunk);
        scratch.mapping.clear();
        (scratch.clean_chunk.as_str(), false)
    } else {
        // Plain path: match directly on the original bytes.
        scratch.mapping.clear();
        (chunk, false)
    };

    // All-visible-bytes could be removed after ANSI stripping.
    if clean_chunk.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let build_elapsed_us = build_started_at.map_or(0, |start| start.elapsed().as_micros());

    let match_stats = collect_chunk_matches(
        MatchCollectionContext {
            clean_chunk,
            chunk_len: chunk.len(),
            use_mapping,
            mapping: &scratch.mapping,
            rules,
            rule_set,
            debug_logging,
            thread_id,
            chunk_id,
        },
        &mut scratch.matches,
    );

    if scratch.matches.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let format_started_at = debug_logging.then(Instant::now);
    let estimated_capacity = chunk
        .len()
        .saturating_add(scratch.matches.len().saturating_mul(reset_color.len().saturating_add(16)));
    scratch.highlighted.clear();
    scratch.highlighted.reserve(estimated_capacity);

    let accepted_match_count = render_highlighted_chunk(
        chunk,
        rules,
        &scratch.matches,
        reset_color,
        should_scan_color_state,
        color_state,
        &mut scratch.highlighted,
    );

    if let Some(thread_id) = thread_id {
        let format_elapsed_us = format_started_at.map_or(0, |start| start.elapsed().as_micros());
        let total_match_elapsed_us: u128 = match_stats
            .rule_timings_ns
            .as_ref()
            .map(|timings| timings.iter().copied().sum::<u128>() / 1000)
            .unwrap_or(0);

        let top_rules = match_stats
            .rule_timings_ns
            .as_ref()
            .map(|timings| top_rule_timing_summary(timings, 5))
            .unwrap_or_default();

        log_debug!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
        log_debug!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
        log_debug!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, scratch.matches);
        log_debug!(
            "[{:?}] Chunk[{:?}] 4:Accepted matches: {}/{}",
            thread_id,
            chunk_id,
            accepted_match_count,
            scratch.matches.len()
        );
        log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, scratch.highlighted);
        log_debug!(
            "[{:?}] Chunk[{:?}] 6:Timings build={}us prefilter={}us match={}us format={}us prefilter_used={} top_rules={}",
            thread_id,
            chunk_id,
            build_elapsed_us,
            match_stats.prefilter_elapsed_us,
            total_match_elapsed_us,
            format_elapsed_us,
            match_stats.prefilter_used,
            top_rules
        );
    }

    // Return borrowed output from scratch to avoid cloning the highlighted chunk.
    Cow::Borrowed(&scratch.highlighted)
}

const MAX_SGR_PARAMS: usize = 24;
const MAX_SGR_RAW_BYTES: usize = 64;

#[derive(Debug, Default)]
struct MatchPhaseStats {
    prefilter_elapsed_us: u128,
    rule_timings_ns: Option<Vec<u128>>,
    prefilter_used: bool,
}

struct MatchCollectionContext<'a> {
    clean_chunk: &'a str,
    chunk_len: usize,
    use_mapping: bool,
    mapping: &'a [usize],
    rules: &'a [CompiledHighlightRule],
    rule_set: Option<&'a RegexSet>,
    debug_logging: bool,
    thread_id: Option<thread::ThreadId>,
    chunk_id: i32,
}

fn collect_chunk_matches(context: MatchCollectionContext<'_>, matches: &mut Vec<(usize, usize, usize)>) -> MatchPhaseStats {
    let MatchCollectionContext {
        clean_chunk,
        chunk_len,
        use_mapping,
        mapping,
        rules,
        rule_set,
        debug_logging,
        thread_id,
        chunk_id,
    } = context;

    matches.clear();
    let mut stats = MatchPhaseStats {
        rule_timings_ns: debug_logging.then(|| vec![0u128; rules.len()]),
        ..MatchPhaseStats::default()
    };

    let mut push_matches = |rule_idx: usize, regex: &Regex| {
        let rule_started_at = debug_logging.then(Instant::now);

        for mat in regex.find_iter(clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            let (raw_start, raw_end) = if use_mapping {
                // For ANSI-cleaned chunks, remap match offsets back to original raw chunk.
                map_clean_range_to_raw(clean_start, clean_end, mapping, chunk_len, thread_id, chunk_id)
            } else {
                (clean_start, clean_end)
            };

            matches.push((raw_start, raw_end, rule_idx));
        }

        if let (Some(start), Some(rule_timings)) = (rule_started_at, stats.rule_timings_ns.as_mut()) {
            rule_timings[rule_idx] = rule_timings[rule_idx].saturating_add(start.elapsed().as_nanos());
        }
    };

    // Prefilter is optional and capped by a rule-count heuristic.
    stats.prefilter_used = rule_set.is_some() && rules.len() <= MAX_RULES_FOR_REGEXSET_PREFILTER;
    if stats.prefilter_used {
        if let Some(prefilter) = rule_set {
            let prefilter_started_at = debug_logging.then(Instant::now);
            let matched_rules = prefilter.matches(clean_chunk);
            for rule_idx in matched_rules.iter() {
                push_matches(rule_idx, &rules[rule_idx].regex);
            }
            stats.prefilter_elapsed_us = prefilter_started_at.map_or(0, |start| start.elapsed().as_micros());
        } else {
            for (rule_idx, rule) in rules.iter().enumerate() {
                push_matches(rule_idx, &rule.regex);
            }
        }
    } else {
        for (rule_idx, rule) in rules.iter().enumerate() {
            push_matches(rule_idx, &rule.regex);
        }
    }

    // Deterministic conflict ordering:
    // 1) earliest start wins
    // 2) when starts are equal, lower rule index (config order) wins
    // 3) final end tie-break keeps ordering total for unstable sort
    matches.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.2.cmp(&right.2)).then(left.1.cmp(&right.1)));

    stats
}

fn render_highlighted_chunk(
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
        // First accepted range wins; later overlapping ranges are discarded.
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

fn sync_color_state_for_chunk(chunk: &str, color_state: &mut AnsiColorState) {
    let mut scan_index = 0usize;
    advance_color_state_to(chunk, &mut scan_index, chunk.len(), color_state);
}

fn analyze_rule_reset_mode(style: &str) -> RuleResetMode {
    let Some((params, params_len)) = parse_rule_sgr_params(style) else {
        return RuleResetMode::Static;
    };

    let (restore_fg, restore_bg) = sgr_params_touch_colors(&params[..params_len]);
    if restore_fg || restore_bg {
        RuleResetMode::Dynamic { restore_fg, restore_bg }
    } else {
        RuleResetMode::Static
    }
}

fn parse_rule_sgr_params(style: &str) -> Option<([u16; MAX_SGR_PARAMS], usize)> {
    let bytes = style.as_bytes();
    if bytes.len() < 3 || bytes[0] != 0x1b || bytes[1] != b'[' || bytes[bytes.len() - 1] != b'm' {
        return None;
    }

    Some(parse_sgr_params(&bytes[2..bytes.len() - 1]))
}

fn push_color_restore_sequence(out: &mut String, color_state: &AnsiColorState, restore_fg: bool, restore_bg: bool) {
    if !restore_fg && !restore_bg {
        return;
    }

    out.push_str("\x1b[");
    let mut wrote_param = false;

    if restore_fg {
        append_color_restore_param(out, color_state.fg, true, &mut wrote_param);
    }
    if restore_bg {
        append_color_restore_param(out, color_state.bg, false, &mut wrote_param);
    }

    out.push('m');
}

fn append_color_restore_param(out: &mut String, color: Option<AnsiColor>, foreground: bool, wrote_param: &mut bool) {
    let mut push_param = |value: u16| {
        if *wrote_param {
            out.push(';');
        }
        let _ = write!(out, "{}", value);
        *wrote_param = true;
    };

    match color {
        Some(AnsiColor::Indexed(value)) => {
            if foreground {
                if value < 8 {
                    push_param(30u16 + value as u16);
                } else if value < 16 {
                    push_param(90u16 + (value - 8) as u16);
                } else {
                    push_param(38);
                    push_param(5);
                    push_param(value as u16);
                }
            } else if value < 8 {
                push_param(40u16 + value as u16);
            } else if value < 16 {
                push_param(100u16 + (value - 8) as u16);
            } else {
                push_param(48);
                push_param(5);
                push_param(value as u16);
            }
        }
        Some(AnsiColor::Rgb(red, green, blue)) => {
            if foreground {
                push_param(38);
            } else {
                push_param(48);
            }
            push_param(2);
            push_param(red as u16);
            push_param(green as u16);
            push_param(blue as u16);
        }
        None => {
            if foreground {
                push_param(39);
            } else {
                push_param(49);
            }
        }
    }
}

fn advance_color_state_to(chunk: &str, scan_index: &mut usize, target: usize, color_state: &mut AnsiColorState) {
    let bytes = chunk.as_bytes();
    let target = target.min(bytes.len());
    let mut index = (*scan_index).min(bytes.len());

    while index < target {
        consume_ansi_byte(bytes[index], color_state);
        index += 1;
    }

    *scan_index = index;
}

fn consume_ansi_byte(byte: u8, color_state: &mut AnsiColorState) {
    let mut apply_params: Option<([u16; MAX_SGR_PARAMS], usize)> = None;

    match &mut color_state.parser_state {
        AnsiParserState::Ground => {
            if byte == 0x1b {
                color_state.parser_state = AnsiParserState::Esc;
            }
        }
        AnsiParserState::Esc => {
            color_state.parser_state = if byte == b'[' {
                AnsiParserState::Csi {
                    raw: [0; MAX_SGR_RAW_BYTES],
                    len: 0,
                }
            } else if byte == 0x1b {
                AnsiParserState::Esc
            } else {
                AnsiParserState::Ground
            };
        }
        AnsiParserState::Csi { raw, len } => {
            if (0x40..=0x7E).contains(&byte) {
                if byte == b'm' {
                    apply_params = Some(parse_sgr_params(&raw[..*len]));
                }
                color_state.parser_state = AnsiParserState::Ground;
            } else if *len < MAX_SGR_RAW_BYTES {
                raw[*len] = byte;
                *len += 1;
            } else {
                // Overlong CSI payloads are ignored to keep parser state bounded.
                color_state.parser_state = if byte == 0x1b { AnsiParserState::Esc } else { AnsiParserState::Ground };
            }
        }
    }

    if let Some((params, params_len)) = apply_params {
        apply_sgr_params(&params[..params_len], color_state);
    }
}

fn parse_sgr_params(raw_params: &[u8]) -> ([u16; MAX_SGR_PARAMS], usize) {
    let mut values = [0u16; MAX_SGR_PARAMS];
    if raw_params.is_empty() {
        values[0] = 0;
        return (values, 1);
    }

    let mut len = 0usize;
    let mut value = 0u16;
    let mut has_digits = false;

    for byte in raw_params.iter().copied() {
        if byte.is_ascii_digit() {
            has_digits = true;
            value = value.saturating_mul(10).saturating_add((byte - b'0') as u16);
            continue;
        }

        if byte == b';' || byte == b':' {
            if len < MAX_SGR_PARAMS {
                values[len] = if has_digits { value } else { 0 };
                len += 1;
            }
            value = 0;
            has_digits = false;
            continue;
        }

        // Unexpected byte in parameter list; stop parsing at this boundary.
        break;
    }

    if len < MAX_SGR_PARAMS {
        values[len] = if has_digits { value } else { 0 };
        len += 1;
    }

    if len == 0 {
        values[0] = 0;
        return (values, 1);
    }

    (values, len)
}

fn sgr_params_touch_colors(params: &[u16]) -> (bool, bool) {
    let mut touches_fg = false;
    let mut touches_bg = false;
    let mut idx = 0usize;

    while idx < params.len() {
        match params[idx] {
            0 => {
                touches_fg = true;
                touches_bg = true;
                idx += 1;
            }
            30..=37 | 90..=97 | 39 => {
                touches_fg = true;
                idx += 1;
            }
            40..=47 | 100..=107 | 49 => {
                touches_bg = true;
                idx += 1;
            }
            38 => {
                touches_fg = true;
                idx += parse_extended_sgr_color(params, idx).step;
            }
            48 => {
                touches_bg = true;
                idx += parse_extended_sgr_color(params, idx).step;
            }
            _ => idx += 1,
        }
    }

    (touches_fg, touches_bg)
}

fn apply_sgr_params(params: &[u16], color_state: &mut AnsiColorState) {
    let mut idx = 0usize;

    while idx < params.len() {
        match params[idx] {
            0 => {
                color_state.fg = None;
                color_state.bg = None;
                idx += 1;
            }
            30..=37 => {
                color_state.fg = Some(AnsiColor::Indexed((params[idx] - 30) as u8));
                idx += 1;
            }
            90..=97 => {
                color_state.fg = Some(AnsiColor::Indexed((params[idx] - 90 + 8) as u8));
                idx += 1;
            }
            39 => {
                color_state.fg = None;
                idx += 1;
            }
            40..=47 => {
                color_state.bg = Some(AnsiColor::Indexed((params[idx] - 40) as u8));
                idx += 1;
            }
            100..=107 => {
                color_state.bg = Some(AnsiColor::Indexed((params[idx] - 100 + 8) as u8));
                idx += 1;
            }
            49 => {
                color_state.bg = None;
                idx += 1;
            }
            38 => {
                let parsed = parse_extended_sgr_color(params, idx);
                if let Some(color) = parsed.color {
                    color_state.fg = Some(color);
                }
                idx += parsed.step;
            }
            48 => {
                let parsed = parse_extended_sgr_color(params, idx);
                if let Some(color) = parsed.color {
                    color_state.bg = Some(color);
                }
                idx += parsed.step;
            }
            _ => idx += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedExtendedColor {
    step: usize,
    color: Option<AnsiColor>,
}

fn parse_extended_sgr_color(params: &[u16], idx: usize) -> ParsedExtendedColor {
    let Some(&selector) = params.get(idx + 1) else {
        return ParsedExtendedColor { step: 1, color: None };
    };

    match selector {
        5 => {
            if params.get(idx + 2).is_some() {
                let indexed = params[idx + 2].min(u8::MAX as u16) as u8;
                ParsedExtendedColor {
                    step: 3,
                    color: Some(AnsiColor::Indexed(indexed)),
                }
            } else {
                ParsedExtendedColor { step: 2, color: None }
            }
        }
        2 => {
            if idx + 4 < params.len() {
                let red = params[idx + 2].min(u8::MAX as u16) as u8;
                let green = params[idx + 3].min(u8::MAX as u16) as u8;
                let blue = params[idx + 4].min(u8::MAX as u16) as u8;
                ParsedExtendedColor {
                    step: 5,
                    color: Some(AnsiColor::Rgb(red, green, blue)),
                }
            } else {
                ParsedExtendedColor {
                    step: params.len().saturating_sub(idx),
                    color: None,
                }
            }
        }
        _ => ParsedExtendedColor { step: 2, color: None },
    }
}

fn map_clean_range_to_raw(
    clean_start: usize,
    clean_end: usize,
    mapping: &[usize],
    raw_len: usize,
    thread_id: Option<thread::ThreadId>,
    chunk_id: i32,
) -> (usize, usize) {
    let raw_start = if clean_start < mapping.len() {
        mapping[clean_start]
    } else {
        if let Some(thread_id) = thread_id {
            log_debug!(
                "[{:?}] Chunk[{:?}] Index mapping fallback: clean_start {} >= mapping.len() {}",
                thread_id,
                chunk_id,
                clean_start,
                mapping.len()
            );
        }
        0
    };

    let raw_end = if clean_end < mapping.len() {
        mapping[clean_end]
    } else {
        if let Some(thread_id) = thread_id {
            log_debug!(
                "[{:?}] Chunk[{:?}] Index mapping fallback: clean_end {} >= mapping.len() {}",
                thread_id,
                chunk_id,
                clean_end,
                mapping.len()
            );
        }
        raw_len
    };

    (raw_start, raw_end)
}

fn build_clean_chunk_no_ansi(raw: &str, clean_chunk: &mut String) {
    clean_chunk.clear();
    clean_chunk.reserve(raw.len());

    for ch in raw.chars() {
        // Keep byte count stable enough for non-ANSI path by replacing line breaks
        // with one visible separator for regex matching.
        if ch == '\n' || ch == '\r' {
            clean_chunk.push(' ');
        } else {
            clean_chunk.push(ch);
        }
    }
}

/// Build a mapping of the original string to a cleaned version with ANSI
/// sequences and newlines removed and return both in reusable buffers.
fn build_index_mapping(raw: &str, clean_chunk: &mut String, mapping: &mut Vec<usize>) {
    let Some(ansi_escape_regex) = ANSI_ESCAPE_REGEX.as_ref() else {
        // Regex compile failure fallback: keep semantics without ANSI stripping.
        build_clean_chunk_no_ansi(raw, clean_chunk);
        mapping.clear();
        mapping.reserve(raw.len().saturating_add(1));
        let mut raw_idx = 0usize;
        for ch in raw.chars() {
            let ch_len = ch.len_utf8();
            for _ in 0..ch_len {
                mapping.push(raw_idx);
            }
            raw_idx = raw_idx.saturating_add(ch_len);
        }
        mapping.push(raw_idx);
        return;
    };

    clean_chunk.clear();
    mapping.clear();
    clean_chunk.reserve(raw.len());
    mapping.reserve(raw.len().saturating_add(1));

    let mut raw_idx = 0usize;
    let mut ansi_iter = ansi_escape_regex.find_iter(raw).peekable();

    for ch in raw.chars() {
        let ch_len = ch.len_utf8();

        while let Some(escape_match) = ansi_iter.peek() {
            if raw_idx >= escape_match.end() {
                ansi_iter.next();
            } else {
                break;
            }
        }

        let in_ansi = ansi_iter
            .peek()
            .map(|escape_match| raw_idx >= escape_match.start() && raw_idx < escape_match.end())
            .unwrap_or(false);

        if !in_ansi {
            let clean_byte_pos = clean_chunk.len();
            if ch == '\n' || ch == '\r' {
                clean_chunk.push(' ');
            } else {
                clean_chunk.push(ch);
            }

            // Each clean byte points to the raw byte where the source char starts.
            let clean_char_len = clean_chunk.len().saturating_sub(clean_byte_pos);
            for _ in 0..clean_char_len {
                mapping.push(raw_idx);
            }
        }

        raw_idx = raw_idx.saturating_add(ch_len);
    }

    mapping.push(raw_idx);
}

fn top_rule_timing_summary(rule_timings_ns: &[u128], limit: usize) -> String {
    let mut indexed: Vec<(usize, u128)> = rule_timings_ns.iter().copied().enumerate().filter(|(_, elapsed_ns)| *elapsed_ns > 0).collect();
    indexed.sort_unstable_by(|left, right| right.1.cmp(&left.1));

    indexed
        .into_iter()
        .take(limit)
        .map(|(idx, elapsed_ns)| format!("r{}={}us", idx, elapsed_ns / 1000))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
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
}
