use crate::vault::commands::{add, del, init, show};
use clap::ArgMatches;
// use keepass::{
//     db::{Entry, Group, Node},
//     error::DatabaseOpenError,
//     Database, DatabaseKey,
// };

pub fn run(matches: &ArgMatches) {
    // First, check if a nested subcommand was provided.
    if let Some((subcommand, sub_matches)) = matches.subcommand() {
        match subcommand {
            "add" => add::run(sub_matches),
            "del" => del::run(sub_matches),
            "init" => init::run(sub_matches),
            "list" => show::run(sub_matches),
            _ => {
                eprintln!("Unknown vault subcommand provided.");
            }
        }
    } else {
        // No subcommand was provided: check for the --unlock or --lock flags.
        if matches.get_flag("unlock") {
            println!("Unlocking vault...");
            // Insert logic for unlocking the vault here.
        } else if matches.get_flag("lock") {
            println!("Locking vault...");
            // Insert logic for locking the vault here.
        }
    }
}
