use clap::{Arg, Command, ArgMatches};
use rpassword;

/// Returns a `clap::Command` for the "add" subcommand,
/// which adds a new vault entry. It requires an entry name and either
/// the `-p` flag (to prompt for a password) or a key file via `--key`.
pub fn add_args() -> Command {
    Command::new("add")
        .about("Add a new vault entry")
        .arg(
            Arg::new("entry_name")
                .help("Name of the vault entry to add")
                .required(true)
                .index(1), // Positional argument for the entry name.
        )
        // The password flag: if provided, prompt the user for the password.
        // This flag is required unless a key file is provided.
        .arg(
            Arg::new("password_flag")
                .short('p')
                .help("Prompt for password to add to the vault entry")
                .action(clap::ArgAction::SetTrue)
                .required_unless_present("key_file"),
        )
        // Optional key file path.
        .arg(
            Arg::new("key_file")
                .short('k')
                .long("key")
                .help("Path to a key file (if provided, password prompt is optional)")
                .num_args(1)
        )
}

/// Processes the "add" subcommand by retrieving the entry name,
/// prompting for a password if the -p flag is set, and then handling
/// the key file (if provided).
pub fn run(matches: &ArgMatches) {
    // Retrieve the required vault entry name.
    let entry_name = matches
        .get_one::<String>("entry_name")
        .expect("Entry name is required");

    // Retrieve the key file path, if provided.
    let key_file = matches.get_one::<String>("key_file");

    // If the -p flag is present, prompt the user for the password.
    let password = if matches.get_flag("password_flag") {
        // Prompt the user to securely enter the password.
        match rpassword::prompt_password("Enter password: ") {
            Ok(pwd) => Some(pwd),
            Err(e) => {
                eprintln!("Error reading password: {}", e);
                None
            }
        }
    } else {
        None
    };

    println!("Adding vault entry: {}", entry_name);
    if let Some(key_file) = key_file {
        println!("Using key file: {}", key_file);
    }
    if let Some(_pwd) = password {
        println!("Password was provided.");
    } else {
        println!("No password was provided.");
    }
    // Insert your logic here to add the vault entry using the provided data.
    
}
