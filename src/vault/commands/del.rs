use clap::ArgMatches;
use std::io::{self, Write};

/// Processes the "del" subcommand by asking the user for confirmation.
/// The user must type the same entry name that was provided at the command line.
pub fn run(matches: &ArgMatches) {
    // Retrieve the vault entry name. This is safe because it's a required argument.
    let entry_name = matches
        .get_one::<String>("entry_name")
        .expect("entry_name is required");

    // Inform the user and prompt for confirmation.
    println!(
        "You are about to delete the vault entry: \"{}\"",
        entry_name
    );
    println!("To confirm deletion, please type the entry name again:");

    // Flush stdout to ensure the prompt is displayed.
    io::stdout().flush().expect("Failed to flush stdout");

    let mut confirmation_input = String::new();
    io::stdin()
        .read_line(&mut confirmation_input)
        .expect("Failed to read input");
    let confirmation_input = confirmation_input.trim();

    // Compare the user's confirmation input with the provided entry name.
    if confirmation_input == entry_name {
        println!("Deleting vault entry: {}", entry_name);
        // Insert your deletion logic here.
    } else {
        println!("Confirmation failed. The provided entry name did not match.");
        // Optionally, you might exit with an error code.
        // std::process::exit(1);
    }
}
