use crate::Result;
use std::process::{Child, ExitCode};

pub(super) fn run_interactive_session(child: Child) -> Result<ExitCode> {
    super::stream::run_interactive_ssh(child)
}
