use super::logging::{APP_VERSION, apply_debug_logging, apply_ssh_logging, flush_debug_logs, resolve_logging_settings, update_session_name_for_logging};
use super::startup::{exit_with_logged_error, initialize_config_or_exit, load_runtime_config_settings, print_title_banner, try_load_interactive_debug_mode};
use crate::{Result, args, auth, config, log, log_debug, log_debug_raw, log_error, log_info, process, tui};
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

    if args.agent_serve {
        auth::agent::run_server().map_err(|err| {
            log_error!("Password vault agent failed: {}", err);
            std::io::Error::other(err.to_string())
        })?;
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(vault_command) = args.vault_command.as_ref() {
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

    log_debug!(
        "Parsed arguments summary: interactive={} ssh_arg_count={} pass_entry_override={} vault_command={} profile_set={} agent_serve={} test_mode={}",
        args.interactive,
        args.ssh_args.len(),
        args.pass_entry.is_some(),
        args.vault_command.is_some(),
        args.profile.is_some(),
        args.agent_serve,
        args.test_mode
    );
    log_debug_raw!("Parsed arguments: {:?}", args);

    print_title_banner(runtime_settings.show_title);

    if logger.is_ssh_logging_enabled() {
        update_session_name_for_logging(&args.ssh_args);
    }

    log_debug!("Starting configuration file watcher");
    let _watcher = config::config_watcher(args.profile.clone());

    log_info!("Launching SSH process handler");
    let exit_code = process::run_ssh_process(args.ssh_args, args.is_non_interactive, args.pass_entry).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        flush_debug_logs(&logger);
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    flush_debug_logs(&logger);
    Ok(exit_code)
}
