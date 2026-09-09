//! Shared SSH argument helpers.
//!
//! These routines capture the CLI semantics we need in multiple places:
//! identifying passthrough-only invocations and extracting the destination host
//! for logging or vault lookups.

const SSH_FLAGS_WITH_VALUES: &str = "bBcDEeFIiJLlmOoPpQRSWw";
const SSH_FLAGS_WITHOUT_VALUES: &str = "46AaCfgKkMNnqstTVvXxYy";
const NON_INTERACTIVE_FLAGS: &str = "GVOQ";

#[derive(Debug)]
pub(crate) struct ParsedSshOption<'a> {
    pub(crate) flag: char,
    pub(crate) value: Option<&'a str>,
}

#[derive(Debug)]
pub(crate) struct ParsedSshArgs<'a> {
    pub(crate) options: Vec<ParsedSshOption<'a>>,
    pub(crate) destination_index: Option<usize>,
    pub(crate) destination_host: Option<String>,
    pub(crate) explicit_destination_user: Option<String>,
}

fn parse_option_token<'a>(arg: &'a str, next_arg: Option<&'a str>) -> (Vec<ParsedSshOption<'a>>, bool, bool) {
    let mut options = Vec::new();
    let mut recognized = true;
    for (offset, flag) in arg[1..].char_indices() {
        if SSH_FLAGS_WITH_VALUES.contains(flag) {
            let value_start = 1 + offset + flag.len_utf8();
            let attached_value = &arg[value_start..];
            let consumes_next = attached_value.is_empty() && next_arg.is_some();
            options.push(ParsedSshOption {
                flag,
                value: if attached_value.is_empty() { next_arg } else { Some(attached_value) },
            });
            return (options, consumes_next, recognized);
        }

        if !SSH_FLAGS_WITHOUT_VALUES.contains(flag) {
            recognized = false;
        }
        options.push(ParsedSshOption { flag, value: None });
    }

    (options, false, recognized)
}

pub(crate) fn parse_ssh_args(ssh_args: &[String]) -> ParsedSshArgs<'_> {
    let mut options = Vec::new();
    let mut destination_index = None;
    let mut destination_host = None;
    let mut explicit_destination_user = None;
    let mut index = 0;

    while index < ssh_args.len() {
        let arg = &ssh_args[index];
        if arg.starts_with('-') {
            let (parsed_options, consumes_next, recognized) = parse_option_token(arg, ssh_args.get(index + 1).map(String::as_str));
            if destination_index.is_some() && !recognized {
                break;
            }
            options.extend(parsed_options);
            index += usize::from(consumes_next) + 1;
            continue;
        }

        if destination_index.is_none() {
            destination_index = Some(index);
            destination_host = Some(
                arg.split_once('@')
                    .map(|(user, host)| {
                        if !user.is_empty() {
                            explicit_destination_user = Some(user.to_string());
                        }
                        host.to_string()
                    })
                    .unwrap_or_else(|| arg.clone()),
            );
            index += 1;
            continue;
        }

        break;
    }

    ParsedSshArgs {
        options,
        destination_index,
        destination_host,
        explicit_destination_user,
    }
}

/// Returns the target host from a forwarded SSH invocation, if present.
pub fn extract_destination_host(ssh_args: &[String]) -> Option<String> {
    parse_ssh_args(ssh_args).destination_host
}

/// Returns `true` when the forwarded SSH arguments should bypass the normal
/// interactive output pipeline.
pub fn is_non_interactive_ssh_invocation(ssh_args: &[String]) -> bool {
    parse_ssh_args(ssh_args)
        .options
        .iter()
        .any(|option| NON_INTERACTIVE_FLAGS.contains(option.flag))
}

#[cfg(test)]
#[path = "../test/args/ssh.rs"]
mod tests;
