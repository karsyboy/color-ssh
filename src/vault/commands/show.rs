use clap::ArgMatches;

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
