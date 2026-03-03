use std::fmt::Write as _;

const MAX_SGR_PARAMS: usize = 24;
const MAX_SGR_RAW_BYTES: usize = 64;

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
    pub(crate) fn should_scan(&self, has_ansi: bool) -> bool {
        has_ansi || !matches!(self.parser_state, AnsiParserState::Ground)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleResetMode {
    Dynamic { restore_fg: bool, restore_bg: bool },
    Static,
}

pub(super) fn sync_color_state_for_chunk(chunk: &str, color_state: &mut AnsiColorState) {
    let mut scan_index = 0usize;
    advance_color_state_to(chunk, &mut scan_index, chunk.len(), color_state);
}

pub(super) fn analyze_rule_reset_mode(style: &str) -> RuleResetMode {
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

pub(super) fn push_color_restore_sequence(out: &mut String, color_state: &AnsiColorState, restore_fg: bool, restore_bg: bool) {
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

pub(super) fn advance_color_state_to(chunk: &str, scan_index: &mut usize, target: usize, color_state: &mut AnsiColorState) {
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
