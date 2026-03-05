//! Runtime dispatch for interactive mode, protocol mode, vault CLI, and agent mode.

use super::logging::{APP_VERSION, apply_debug_logging, apply_ssh_logging, flush_debug_logs, resolve_logging_settings, update_session_name_for_logging};
use super::startup::{exit_with_logged_error, initialize_config_or_exit, load_runtime_config_settings, print_title_banner, try_load_interactive_debug_mode};
use crate::{Result, args, auth, config, inventory, log, log_debug, log_debug_raw, log_error, log_info, process, tui};
use std::process::ExitCode;

fn run_interactive_session(logger: &log::Logger, args: &args::MainArgs) -> Result<ExitCode> {
    log_info!("Launching interactive session manager");

    let debug_from_config = try_load_interactive_debug_mode(args.profile.clone());
    let (final_debug, _) = resolve_logging_settings(args, debug_from_config, false);
    apply_debug_logging(logger, args, final_debug, debug_from_config);

    if let Err(err) = tui::run_session_manager() {
        exit_with_logged_error(logger, format!("Session manager error: {err}"));
    }

    flush_debug_logs(logger);
    Ok(ExitCode::SUCCESS)
}

fn run_agent_serve() -> Result<ExitCode> {
    auth::agent::run_server().map_err(|err| {
        log_error!("Password vault agent failed: {}", err);
        std::io::Error::other(err.to_string())
    })?;
    Ok(ExitCode::SUCCESS)
}

fn run_inventory_migration() -> Result<ExitCode> {
    let summary = inventory::migrate_default_ssh_config_to_inventory().map_err(|err| {
        log_error!("Inventory migration failed: {}", err);
        std::io::Error::other(err.to_string())
    })?;

    println!("Inventory migration completed.");
    println!("Hosts written: {}", summary.hosts_written);
    println!("Wildcard aliases skipped: {}", summary.wildcard_aliases_skipped);
    println!("Unsupported Match blocks skipped: {}", summary.unsupported_blocks_skipped);
    if let Some(backup_path) = summary.backup_path {
        println!("Backup created: {}", backup_path.display());
    }
    println!("Output path: {}", summary.output_path.display());
    Ok(ExitCode::SUCCESS)
}

fn run_completion_hosts(protocol: &args::CompletionProtocol) -> ExitCode {
    let tree = match inventory::load_inventory_tree() {
        Ok(tree) => tree,
        Err(err) => {
            log_debug!("Skipping completion host listing because inventory could not be loaded: {}", err);
            return ExitCode::SUCCESS;
        }
    };

    let mut host_names: Vec<String> = tree
        .hosts
        .into_iter()
        .filter(|host| !host.hidden)
        .filter(|host| match protocol {
            args::CompletionProtocol::All => true,
            args::CompletionProtocol::Ssh => matches!(host.protocol, inventory::ConnectionProtocol::Ssh),
            args::CompletionProtocol::Rdp => matches!(host.protocol, inventory::ConnectionProtocol::Rdp),
        })
        .map(|host| host.name)
        .collect();

    host_names.sort_by(|left, right| {
        let left_key = left.to_ascii_lowercase();
        let right_key = right.to_ascii_lowercase();
        left_key.cmp(&right_key).then_with(|| left.cmp(right))
    });
    host_names.dedup();

    for host in host_names {
        println!("{host}");
    }
    ExitCode::SUCCESS
}

fn run_vault_mode(logger: &log::Logger, args: &args::MainArgs, vault_command: &args::VaultCommand) -> ExitCode {
    initialize_config_or_exit(logger, args.profile.clone(), "Failed to initialize config for vault command");
    auth::run_vault_command(vault_command)
}

fn configure_non_interactive_runtime(logger: &log::Logger, args: &args::MainArgs) -> super::startup::RuntimeConfigSettings {
    initialize_config_or_exit(logger, args.profile.clone(), "Failed to initialize config");

    let runtime_settings = load_runtime_config_settings();
    let (final_debug, final_ssh_log) = resolve_logging_settings(args, runtime_settings.debug_mode, runtime_settings.ssh_logging);
    apply_debug_logging(logger, args, final_debug, runtime_settings.debug_mode);
    apply_ssh_logging(logger, args, final_ssh_log);
    runtime_settings
}

fn log_argument_summary(args: &args::MainArgs) {
    let ssh_arg_count = match args.command.as_ref() {
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Ssh(ssh_command))) => ssh_command.ssh_args.len(),
        _ => 0,
    };
    let rdp_launch = matches!(args.command, Some(args::MainCommand::Protocol(args::ProtocolCommand::Rdp(_))));
    let vault_command = matches!(args.command, Some(args::MainCommand::Vault(_)));
    let agent_serve = matches!(args.command, Some(args::MainCommand::AgentServe));
    let migrate_inventory = matches!(args.command, Some(args::MainCommand::MigrateInventory));
    let completion_hosts = matches!(args.command, Some(args::MainCommand::CompletionHosts(_)));
    log_debug!(
        "Parsed arguments summary: interactive={} ssh_arg_count={} rdp_launch={} pass_entry_override={} vault_command={} profile_set={} agent_serve={} migrate_inventory={} completion_hosts={} test_mode={}",
        args.interactive,
        ssh_arg_count,
        rdp_launch,
        args.pass_entry.is_some(),
        vault_command,
        args.profile.is_some(),
        agent_serve,
        migrate_inventory,
        completion_hosts,
        args.test_mode
    );
    log_debug_raw!("Parsed arguments: {:?}", args);
}

fn update_protocol_session_name_if_needed(logger: &log::Logger, command: Option<&args::MainCommand>) {
    if !logger.is_ssh_logging_enabled() {
        return;
    }

    match command {
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Rdp(rdp_command))) => {
            update_session_name_for_logging(Some(&rdp_command.target), &[]);
        }
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Ssh(ssh_command))) => {
            update_session_name_for_logging(None, &ssh_command.ssh_args);
        }
        _ => {}
    }
}

fn run_protocol_command(command: args::ProtocolCommand, pass_entry: Option<String>) -> Result<ExitCode> {
    match command {
        args::ProtocolCommand::Rdp(rdp_command) => {
            log_info!("Launching RDP process handler");
            process::run_rdp_process(rdp_command, pass_entry)
        }
        args::ProtocolCommand::Ssh(ssh_command) => {
            log_info!("Launching SSH process handler");
            process::run_ssh_process(ssh_command.ssh_args, ssh_command.is_non_interactive, pass_entry)
        }
    }
}

pub(crate) fn run() -> Result<ExitCode> {
    if auth::transport::is_internal_askpass_invocation() {
        return Ok(auth::run_internal_askpass());
    }

    let args = args::main_args();
    let logger = log::Logger::new();

    if args.debug_count > 0 {
        logger.enable_debug_with_verbosity(log::DebugVerbosity::from_count(args.debug_count));
    }
    log_info!("color-ssh {} starting", APP_VERSION);

    if matches!(args.command, Some(args::MainCommand::AgentServe)) {
        return run_agent_serve();
    }

    if let Some(args::MainCommand::CompletionHosts(protocol)) = args.command.as_ref() {
        return Ok(run_completion_hosts(protocol));
    }


    if matches!(args.command, Some(args::MainCommand::MigrateInventory)) {
        return run_inventory_migration();
    }

    if let Some(args::MainCommand::Vault(vault_command)) = args.command.as_ref() {
        return Ok(run_vault_mode(&logger, &args, vault_command));
    }

    if args.interactive {
        return run_interactive_session(&logger, &args);
    }

    let runtime_settings = configure_non_interactive_runtime(&logger, &args);
    log_argument_summary(&args);

    print_title_banner(runtime_settings.show_title);
    update_protocol_session_name_if_needed(&logger, args.command.as_ref());

    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    let Some(args::MainCommand::Protocol(protocol_command)) = args.command.clone() else {
        unreachable!("non-interactive dispatch requires a protocol command");
    };

    let exit_code = run_protocol_command(protocol_command, args.pass_entry.clone()).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        flush_debug_logs(&logger);
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    flush_debug_logs(&logger);
    Ok(exit_code)
}
