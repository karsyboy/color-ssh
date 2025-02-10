use clap::{Arg, Command, ArgMatches};

/// Returns a `clap::Command` for the "show" subcommand,
/// which is used to display a specific vault entry.
pub fn list_args() -> Command {
    Command::new("show")
        .about("Show a vault entry")
        .arg(
            Arg::new("entry_name")
                .help("Name of the vault entry to show")
                .required(true)
                .index(1), // Positional argument at index 1
        )
}

/// Processes the "show" subcommand.
/// It retrieves the vault entry name from the command-line arguments and prints it.
pub fn run(matches: &ArgMatches) {
    
    // Retrieve the vault entry name; this is safe because it's a required argument.
    let entry_name = matches
        .get_one::<String>("entry_name")
        .expect("Entry name is required");

    // Simulate printing the vault entry.
    println!("Vault entry: {}", entry_name);
}