//! Shared SSH argument helpers.
//!
//! These routines capture the CLI semantics we need in multiple places:
//! identifying passthrough-only invocations and extracting the destination host
//! for logging or vault lookups.

const SSH_FLAGS_WITH_SEPARATE_VALUES: &[&str] = &[
    "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
];

const NON_INTERACTIVE_FLAGS: &[&str] = &["-G", "-V", "-O", "-Q"];

/// Returns the target host from a forwarded SSH invocation, if present.
pub fn extract_destination_host(ssh_args: &[String]) -> Option<String> {
    let mut skip_next = false;

    for arg in ssh_args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg.starts_with('-') {
            if SSH_FLAGS_WITH_SEPARATE_VALUES.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }

        return Some(arg.split_once('@').map_or_else(|| arg.clone(), |(_, host)| host.to_string()));
    }

    None
}

/// Returns `true` when the forwarded SSH arguments should bypass the normal
/// interactive output pipeline.
pub fn is_non_interactive_ssh_invocation(ssh_args: &[String]) -> bool {
    ssh_args.iter().any(|arg| NON_INTERACTIVE_FLAGS.contains(&arg.as_str()))
}

#[cfg(test)]
#[path = "../test/ssh_args.rs"]
mod tests;
