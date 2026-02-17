//! OSC 52 clipboard sequence forwarding for PTY output.

use std::io::{self, Write};

/// Scan PTY output for OSC 52 clipboard sequences and forward them to stdout.
///
/// OSC 52 format: `ESC ] 52 ; <selection> ; <base64-data> BEL` or
/// `ESC ] 52 ; <selection> ; <base64-data> ESC \\`.
/// `osc_buf` accumulates partial sequences across read boundaries.
pub(crate) fn forward_osc52(data: &[u8], osc_buf: &mut Vec<u8>) {
    let forwarded_sequences = collect_osc52_sequences(data, osc_buf);
    if forwarded_sequences.is_empty() {
        return;
    }

    let mut stdout = io::stdout();
    for sequence in forwarded_sequences {
        let _ = stdout.write_all(&sequence);
    }
    let _ = stdout.flush();
}

pub(crate) fn collect_osc52_sequences(data: &[u8], osc_buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut forwarded = Vec::new();
    let mut owned_input: Option<Vec<u8>> = None;
    let mut input = owned_input.as_deref().unwrap_or(data);

    loop {
        if !osc_buf.is_empty() {
            osc_buf.extend_from_slice(input);
            if let Some(end) = find_osc_end(osc_buf) {
                forwarded.push(osc_buf[..end].to_vec());
                owned_input = Some(osc_buf[end..].to_vec());
                osc_buf.clear();
                if owned_input.as_ref().is_some_and(Vec::is_empty) {
                    return forwarded;
                }
                input = owned_input.as_deref().unwrap_or(&[]);
                continue;
            }

            if osc_buf.len() > 100_000 {
                osc_buf.clear();
            }
            return forwarded;
        }

        let mut scan_idx = 0;
        while scan_idx < input.len() {
            if input[scan_idx] == 0x1b && scan_idx + 1 < input.len() && input[scan_idx + 1] == b']' {
                let rest = &input[scan_idx + 2..];
                if rest.starts_with(b"52;") {
                    let seq_start = scan_idx;
                    if let Some(end_offset) = find_osc_end(&input[seq_start..]) {
                        forwarded.push(input[seq_start..seq_start + end_offset].to_vec());
                        scan_idx = seq_start + end_offset;
                        continue;
                    }

                    osc_buf.extend_from_slice(&input[seq_start..]);
                    return forwarded;
                }
            }
            scan_idx += 1;
        }

        return forwarded;
    }
}

/// Find the end of an OSC sequence (after BEL or ST terminator), returning
/// the byte position after the terminator.
pub(crate) fn find_osc_end(data: &[u8]) -> Option<usize> {
    for byte_idx in 0..data.len() {
        if data[byte_idx] == 0x07 {
            // BEL terminator
            return Some(byte_idx + 1);
        }
        if data[byte_idx] == 0x1b && byte_idx + 1 < data.len() && data[byte_idx + 1] == b'\\' {
            // ST terminator (ESC \\)
            return Some(byte_idx + 2);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{collect_osc52_sequences, find_osc_end};

    #[test]
    fn find_osc_end_supports_bel_and_st() {
        assert_eq!(find_osc_end(b"\x1b]52;c;abc\x07"), Some(11));
        assert_eq!(find_osc_end(b"\x1b]52;c;abc\x1b\\"), Some(12));
    }

    #[test]
    fn collect_osc52_sequences_handles_chunked_input_iteratively() {
        let mut osc_buf = Vec::new();
        let first = collect_osc52_sequences(b"\x1b]52;c;Zm9v", &mut osc_buf);
        assert!(first.is_empty());
        assert!(!osc_buf.is_empty());

        let second = collect_osc52_sequences(b"YmFy\x07after", &mut osc_buf);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0], b"\x1b]52;c;Zm9vYmFy\x07".to_vec());
        assert!(osc_buf.is_empty());
    }
}
