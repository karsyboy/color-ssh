use crate::Result;
use std::process::{Child, ExitCode};

pub(super) fn run_interactive_ssh_session(child: Child) -> Result<ExitCode> {
    super::stream::run_interactive_ssh(child)
}

pub(super) fn run_interactive_rdp_session(child: Child) -> Result<ExitCode> {
    super::stream::run_interactive_rdp(child)
}
