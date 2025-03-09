/*
TODO:
    - Develop process.rs to handle subprocess interaction. Write the code so that the interaction can handle multiple subprocesses to expand support.
        - Subprocesses to handle:
            - SSH
            - Telnet
            - Console
            - SSHPASS
*/
use crate::{Result, config, highlighter, log_debug, log_ssh};
use std::{
    io::{self, BufReader, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub fn process_handler(process_args: Vec<String>) -> Result<ExitCode> {
    // Launch the SSH process with the provided arguments
    log_debug!("SSH arguments: {:?}", process_args);
    let mut child = spawn_ssh(&process_args).expect("Failed to spawn SSH process\r");
    let stdout = child.stdout.take().expect("Failed to capture stdout\r");
    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    // Create needed values for chunk reading thread
    let reset_color = "\x1b[0m"; // ANSI reset color sequence

    // Spawn thread for reading chunks sent by ssh
    // This is the main thread that reads the chunks and applies the highlighter
    thread::spawn(move || {
        let mut chunk_id = 0;
        while let Ok(chunk) = rx.recv() {
            let rules = config::SESSION_CONFIG.read().unwrap().metadata.compiled_rules.clone();
            let processed = highlighter::process_chunk(chunk, chunk_id, &rules, reset_color);
            chunk_id += 1;
            print!("{}", processed); // Print the processed chunk
            io::stdout().flush().unwrap(); // Flush to ensure immediate display
        }
    });

    // Buffer for reading data from SSH output
    let mut buffer = [0; 4096];
    loop {
        let n = reader.read(&mut buffer).expect("Failed to read data from SSH process\r");
        if n == 0 {
            break; // Exit loop when EOF is reached
        }
        // Convert the read data to a String and send it to the processing thread
        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
        log_ssh!("{}", chunk);
        tx.send(chunk).expect("Failed to send data to processing thread\r");
    }

    // Wait for the SSH process to finish and use its status code
    let status = child.wait().expect("Failed to wait for SSH process\r");

    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
    }
}

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
