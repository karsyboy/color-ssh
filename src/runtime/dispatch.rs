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
        auth::agent::run_server().map_err(|err| {
            log_error!("Password vault agent failed: {}", err);
            std::io::Error::other(err.to_string())
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    if matches!(args.command, Some(args::MainCommand::MigrateInventory)) {
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
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(args::MainCommand::Vault(vault_command)) = args.command.as_ref() {
        initialize_config_or_exit(&logger, args.profile.clone(), "Failed to initialize config for vault command");
        return Ok(auth::run_vault_command(vault_command));
    }

    if args.interactive {
        return run_interactive_session(&logger, &args);
    }

    initialize_config_or_exit(&logger, args.profile.clone(), "Failed to initialize config");

    let runtime_settings = load_runtime_config_settings();
    let (final_debug, final_ssh_log) = resolve_logging_settings(&args, runtime_settings.debug_mode, runtime_settings.ssh_logging);
    apply_debug_logging(&logger, &args, final_debug, runtime_settings.debug_mode);
    apply_ssh_logging(&logger, &args, final_ssh_log);

    let ssh_arg_count = match args.command.as_ref() {
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Ssh(ssh_command))) => ssh_command.ssh_args.len(),
        _ => 0,
    };
    let rdp_launch = matches!(args.command, Some(args::MainCommand::Protocol(args::ProtocolCommand::Rdp(_))));
    let vault_command = matches!(args.command, Some(args::MainCommand::Vault(_)));
    let agent_serve = matches!(args.command, Some(args::MainCommand::AgentServe));
    let migrate_inventory = matches!(args.command, Some(args::MainCommand::MigrateInventory));

    log_debug!(
        "Parsed arguments summary: interactive={} ssh_arg_count={} rdp_launch={} pass_entry_override={} vault_command={} profile_set={} agent_serve={} migrate_inventory={} test_mode={}",
        args.interactive,
        ssh_arg_count,
        rdp_launch,
        args.pass_entry.is_some(),
        vault_command,
        args.profile.is_some(),
        agent_serve,
        migrate_inventory,
        args.test_mode
    );
    log_debug_raw!("Parsed arguments: {:?}", args);

    print_title_banner(runtime_settings.show_title);

    if logger.is_ssh_logging_enabled() {
        match args.command.as_ref() {
            Some(args::MainCommand::Protocol(args::ProtocolCommand::Rdp(rdp_command))) => {
                update_session_name_for_logging(Some(&rdp_command.target), &[]);
            }
            Some(args::MainCommand::Protocol(args::ProtocolCommand::Ssh(ssh_command))) => {
                update_session_name_for_logging(None, &ssh_command.ssh_args);
            }
            _ => {}
        }
    }

    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    let exit_code = match args.command.clone() {
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Rdp(rdp_command))) => {
            log_info!("Launching RDP process handler");
            process::run_rdp_process(rdp_command, args.pass_entry.clone())
        }
        Some(args::MainCommand::Protocol(args::ProtocolCommand::Ssh(ssh_command))) => {
            log_info!("Launching SSH process handler");
            process::run_ssh_process(ssh_command.ssh_args, ssh_command.is_non_interactive, args.pass_entry.clone())
        }
        _ => unreachable!("non-interactive dispatch requires a protocol command"),
    }
    .map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        flush_debug_logs(&logger);
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    flush_debug_logs(&logger);
    Ok(exit_code)
}
