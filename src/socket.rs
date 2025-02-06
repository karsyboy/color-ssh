#[cfg(unix)]
// Socket module to be used if OS is detected as a unix based OS
pub mod unix_socket {
    use once_cell::sync::Lazy;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::thread;

    // Global value created for PID and SOCKET_PATH to make calls eiser throughout modules and so that send_command* functions dont need to figure out the path or have it passed
    const PID: Lazy<u32> = Lazy::new(|| {
        return std::process::id();
    });

    const SOCKET_PATH: Lazy<String> = Lazy::new(|| {
        return format!("/tmp/csh_reload_{}.sock", *PID);
    });

    /// Creats the socket and starts a thread for monitoring if any commands are sent.
    /// reload_callback is sent if the reload command is recived so that the main functions knows to execute the code needed to reload the config file
    pub fn start_socket_listener<F>(reload_callback: F)
    where
        F: Fn() + Send + 'static,
    {
        if Path::new(&*SOCKET_PATH).exists() {
            fs::remove_file(&*SOCKET_PATH).expect("Failed to remove old socket file");
        }

        let listener = UnixListener::bind(&*SOCKET_PATH)
            .unwrap_or_else(|e| panic!("Failed to bind to socket {}: {}", *SOCKET_PATH, e));
        println!("Instance (pid {}) listening on {}", *PID, *SOCKET_PATH);

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let mut command = String::new();
                        let mut reader = BufReader::new(&stream);
                        if reader.read_line(&mut command).is_ok() {
                            let command = command.trim();
                            match command {
                                "reload" => {
                                    println!(
                                        "\rInstance (pid {}) received reload command.\r",
                                        *PID
                                    );
                                    reload_callback();
                                    let response = format!("reload successful: {}\n", *PID);
                                    let _ = stream.write_all(response.as_bytes());
                                }
                                "exit" => {
                                    println!("Instance (pid {}) received exit command.", *PID);
                                    let response = format!("exit successful: {}\n", *PID);
                                    let _ = stream.write_all(response.as_bytes());
                                    let _ = fs::remove_file(&*SOCKET_PATH);
                                    std::process::exit(0);
                                }
                                _ => {
                                    println!(
                                        "Instance (pid {}) received unknown command: {}",
                                        *PID, command
                                    );
                                    let _ = stream.write_all(b"unknown command\n");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Instance (pid {}) failed to accept connection: {}", *PID, e)
                    }
                }
            }
        });
    }

    /// Recived a command from another instance of csh and then runs that command agianst all active sockets
    /// 
    ///  - `command`: A string that is sent as the command to the socket
    /// 
    /// Exectutes the command and then retruns back to the main function
    pub fn send_command_to_all(command: &str) {
        let tmp_dir = "/tmp";
        let entries = fs::read_dir(tmp_dir).expect("Failed to read /tmp directory");

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                if fname.starts_with("csh_reload_") && fname.ends_with(".sock") {
                    match UnixStream::connect(&path) {
                        Ok(mut stream) => {
                            let _ = stream.write_all(format!("{}\n", command).as_bytes());
                            let mut reader = BufReader::new(&stream);
                            let mut response = String::new();
                            if let Ok(_) = reader.read_line(&mut response) {
                                println!("Response from {:?}: {}", path, response.trim());
                            }
                        }
                        Err(e) => eprintln!("Failed to connect to socket {:?}: {}", path, e),
                    }
                }
            }
        }

        println!(
            "Command `{}` sent to all instances; exiting gracefully.",
            command
        );
        std::process::exit(0);
    }

    /// Recives a command and the runs it only on the instance of csh the sent the command.
    ///
    ///  - `command`: A string that is sent as the command to the socket
    /// 
    /// Exectutes the command and then retruns back to the main function
    pub fn send_command(command: &str) {
        match UnixStream::connect(&*SOCKET_PATH) {
            Ok(mut stream) => {
                let _ = stream.write_all(format!("{}\n", command).as_bytes());
                let mut reader = BufReader::new(&stream);
                let mut response = String::new();
                if let Ok(_) = reader.read_line(&mut response) {
                    println!("Response from {:?}: {}", *SOCKET_PATH, response.trim());
                }
            }
            Err(e) => eprintln!("Failed to connect to socket {:?}: {}", *SOCKET_PATH, e),
        }
    }
}

#[cfg(windows)]
pub mod windows_pipe {
    use once_cell::sync::Lazy;
    use std::fs::{self, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::named_pipe::{NamedPipeClient, NamedPipeServer};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    const PIPE_DIR: &str = ".csh";
    const PIPE_LIST_FILE: &str = ".csh/csh_pipe";

    // Global value created for PID and PIPE_NAME to make calls eiser throughout modules and so that send_command* functions dont need to figure out the pipe or have it passed
    static PID: Lazy<u32> = Lazy::new(|| {
        return std::process::id();
    });

    static PIPE_NAME: Lazy<String> = Lazy::new(|| return format!(r"\\.\pipe\csh_reload_{}", *PID));

    fn get_pipe_list_path() -> PathBuf {
        dirs::home_dir().unwrap().join(PIPE_LIST_FILE)
    }

    /// Creats the socket and starts a thread for monitoring if any commands are sent.
    /// reload_callback is sent if the reload command is recived so that the main functions knows to execute the code needed to reload the config file
    pub fn start_socket_listener<F>(reload_callback: F)
    where
        F: Fn() + Send + 'static,
    {
        let pipe_list_path = get_pipe_list_path();

        // Ensure .csh directory exists
        let pipe_dir = dirs::home_dir().unwrap().join(PIPE_DIR);
        if !pipe_dir.exists() {
            fs::create_dir_all(&pipe_dir).expect("Failed to create .csh directory");
        }

        // Register this pipe in the csh_pipe file
        {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&pipe_list_path)
                .expect("Failed to open pipe list file");
            writeln!(file, "{}", *PIPE_NAME).expect("Failed to write to pipe list");
        }

        println!("Instance (pid {}) listening on {}", *PID, *PIPE_NAME);

        let running = Arc::new(Mutex::new(true));
        let running_clone = Arc::clone(&running);

        thread::spawn(move || {
            while *running_clone.lock().unwrap() {
                if let Ok(mut server) = NamedPipeServer::create(&*PIPE_NAME) {
                    let mut reader = BufReader::new(&server);
                    let mut command = String::new();
                    if reader.read_line(&mut command).is_ok() {
                        let command = command.trim();
                        match command {
                            "reload" => {
                                println!("Instance (pid {}) received reload command.\r", *PID);
                                reload_callback();
                                let response = format!("reload successful: {}\n", *PID);
                                let _ = server.write_all(response.as_bytes());
                            }
                            "exit" => {
                                println!("Instance (pid {}) received exit command.", *PID);
                                let response = format!("exit successful: {}\n", *PID);
                                let _ = server.write_all(response.as_bytes());

                                // Remove pipe entry from csh_pipe file
                                let mut pipes = fs::read_to_string(&pipe_list_path)
                                    .unwrap_or_default()
                                    .lines()
                                    .filter(|p| *p != *PIPE_NAME)
                                    .map(|s| s.to_string())
                                    .collect::<Vec<String>>();
                                fs::write(&pipe_list_path, pipes.join("\n"))
                                    .expect("Failed to update pipe list");

                                *running_clone.lock().unwrap() = false;
                                break;
                            }
                            _ => {
                                println!(
                                    "Instance (pid {}) received unknown command: {}",
                                    *PID, command
                                );
                                let _ = server.write_all(b"unknown command\n");
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
    }

    /// Recived a command from another instance of csh and then runs that command agianst all active sockets
    /// 
    ///  - `command`: A string that is sent as the command to the socket
    /// 
    /// Exectutes the command and then retruns back to the main function
    pub fn send_command_to_all(command: &str) {
        let pipe_list_path = get_pipe_list_path();

        let pipes = fs::read_to_string(&pipe_list_path).unwrap_or_default();
        for pipe in pipes.lines() {
            if pipe.is_empty() {
                continue;
            }

            match NamedPipeClient::connect(pipe) {
                Ok(mut client) => {
                    let _ = client.write_all(format!("{}\n", command).as_bytes());
                    let mut reader = BufReader::new(&client);
                    let mut response = String::new();
                    if let Ok(_) = reader.read_line(&mut response) {
                        println!("Response from {}: {}", pipe, response.trim());
                    }
                }
                Err(e) => eprintln!("Failed to connect to {}: {}", pipe, e),
            }
        }

        println!(
            "Command `{}` sent to all instances; exiting gracefully.",
            command
        );
        std::process::exit(0);
    }

    /// Recives a command and the runs it only on the instance of csh the sent the command.
    ///
    ///  - `command`: A string that is sent as the command to the socket
    /// 
    /// Exectutes the command and then retruns back to the main function
    pub fn send_command(command: &str) {
        match NamedPipeClient::connect(*PIPE_NAME) {
            Ok(mut client) => {
                let _ = client.write_all(format!("{}\n", command).as_bytes());
                let mut reader = BufReader::new(&client);
                let mut response = String::new();
                if let Ok(_) = reader.read_line(&mut response) {
                    println!("Response from {}: {}", *PIP_NAME, response.trim());
                }
            }
            Err(e) => eprintln!("Failed to connect to {}: {}", *PIPE_NAME, e),
        }
    }
}
