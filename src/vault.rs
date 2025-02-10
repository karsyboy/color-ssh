use clap::{Arg, ArgMatches, Command};
use crate::vault_cmds::{add,del,init,show};

pub fn vault_args() -> Command {
    Command::new("vault")
        .about("Interact with CSH credential vault")
        .arg_required_else_help(true)
        
        // Nested subcommands from their own modules:
        .subcommand(add::add_args())
        .subcommand(del::del_args())
        .subcommand(init::init_args())
        .subcommand(show::list_args())

        // Additional flags that are valid when no subcommand is used:
        .arg(
            Arg::new("unlock")
                .long("unlock")
                .help("Unlock Vault")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("lock")
                .long("lock")
                .help("Lock Vault")
                .action(clap::ArgAction::SetTrue),
        )
}

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
