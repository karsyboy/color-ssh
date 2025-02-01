use std::process::{Command, Stdio};

/// Spawns an SSH process with the provided arguments.
///
///  `args`: CLI arguments provided by the user.
///
/// Returns the spawned child process.
pub fn spawn_ssh(args: &[String]) -> std::io::Result<std::process::Child> {
    let child = Command::new("ssh")
        .args(args)
        .stdin(Stdio::inherit()) // Inherit the input from the current terminal
        .stdout(Stdio::piped()) // Pipe the output for processing
        .stderr(Stdio::inherit()) // Inherit the error stream from the SSH process
        .spawn()?;
    Ok(child)
}
