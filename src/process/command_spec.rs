use crate::auth::secret::SensitiveString;
use crate::command_path;
use std::io;
use std::process::Command;

#[derive(Debug)]
pub(crate) struct PreparedCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) stdin_payload: Option<SensitiveString>,
    pub(crate) fallback_notice: Option<String>,
}

impl PreparedCommand {
    pub(crate) fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            env: Vec::new(),
            stdin_payload: None,
            fallback_notice: None,
        }
    }
}

pub(crate) fn build_plain_ssh_command(args: &[String]) -> PreparedCommand {
    PreparedCommand::new("ssh", args.to_vec())
}

pub(crate) fn command_from_spec(spec: &PreparedCommand) -> io::Result<Command> {
    let program_path = command_path::resolve_known_command_path(&spec.program)?;
    let mut command = Command::new(&program_path);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    Ok(command)
}
