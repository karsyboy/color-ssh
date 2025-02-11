use clap::{Arg, ArgMatches, Command};

/// Returns a `clap::Command` for the "init" subcommand,
/// which initializes a CSH vault and requires a vault name.
pub fn init_args() -> Command {
    Command::new("init").about("Initialize CSH vault").arg(
        Arg::new("vault_name")
            .help("Name of the vault to initialize")
            .required(true)
            .index(1), // Positional argument at index 1.
    )
}

/// Processes the "init" subcommand.
pub fn run(matches: &ArgMatches) {
    // Retrieve the vault name from the matches.
    let vault_name = matches
        .get_one::<String>("vault_name")
        .expect("Vault name is required.");

    println!("Initializing vault: {}", vault_name);
    // Insert logic to initialize the vault here.
}
