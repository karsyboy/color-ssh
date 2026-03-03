//! Top-level application startup and command dispatch.

use crate::{Result, args, auth, config, log, log_debug, log_debug_raw, log_error, log_info, log_warn, process, ssh_args, tui};
use std::process::ExitCode;
use std::sync::Once;

const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));
const TITLE_BANNER: &[&str] = &[
    " ",
    "\x1b[31m ██████╗ ██████╗ ██╗      ██████╗ ██████╗       ███████╗███████╗██╗  ██╗",
    "\x1b[33m██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗      ██╔════╝██╔════╝██║  ██║",
    "\x1b[32m██║     ██║   ██║██║     ██║   ██║██████╔╝█████╗███████╗███████╗███████║",
    "\x1b[36m██║     ██║   ██║██║     ██║   ██║██╔══██╗╚════╝╚════██║╚════██║██╔══██║",
    "\x1b[34m╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║      ███████║███████║██║  ██║",
    "\x1b[35m ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝      ╚══════╝╚══════╝╚═╝  ╚═╝",
    concat!(
        "\x1b[31mVersion: \x1b[33mv",
        env!("CARGO_PKG_VERSION"),
        "\x1b[0m    \x1b[31mBy: \x1b[32m@Karsyboy\x1b[0m    \x1b[31mGithub: \x1b[34mhttps://github.com/karsyboy/color-ssh\x1b[0m"
    ),
    " ",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DebugModeSource {
    CliSafe,
    CliRaw,
    ConfigSafe,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeConfigSettings {
    debug_mode: bool,
    ssh_logging: bool,
    show_title: bool,
}

pub(crate) fn resolve_logging_settings(args: &args::MainArgs, debug_from_config: bool, ssh_log_from_config: bool) -> (log::DebugVerbosity, bool) {
    let cli_debug = log::DebugVerbosity::from_count(args.debug_count);
    let config_debug = if debug_from_config {
        log::DebugVerbosity::Safe
    } else {
        log::DebugVerbosity::Off
    };

    if args.test_mode {
        (cli_debug, args.ssh_logging)
    } else {
        (cli_debug.max(config_debug), args.ssh_logging || ssh_log_from_config)
    }
}

fn debug_mode_source(args: &args::MainArgs, debug_from_config: bool) -> Option<DebugModeSource> {
    match args.debug_count {
        2.. => Some(DebugModeSource::CliRaw),
        1 => Some(DebugModeSource::CliSafe),
        _ if debug_from_config => Some(DebugModeSource::ConfigSafe),
        _ => None,
    }
}

fn emit_raw_debug_warning_once() {
    static RAW_DEBUG_WARNING: Once = Once::new();
    const RAW_DEBUG_WARNING_MESSAGE: &str =
        "Raw debug logging is enabled and may capture terminal content, CLI arguments, and secrets in ~/.color-ssh/logs/cossh.log.";

    RAW_DEBUG_WARNING.call_once(|| {
        eprintln!("[color-ssh] {}", RAW_DEBUG_WARNING_MESSAGE);
        log_warn!("{}", RAW_DEBUG_WARNING_MESSAGE);
    });
}

fn flush_debug_logs(logger: &log::Logger) {
    let _ = logger.flush_debug();
}

fn exit_with_logged_error(logger: &log::Logger, message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    flush_debug_logs(logger);
    std::process::exit(1);
}

fn initialize_config_or_exit(logger: &log::Logger, profile: Option<String>, context: &str) {
    if let Err(err) = config::init_session_config(profile) {
        log_error!("{context}: {}", err);
        exit_with_logged_error(logger, format!("Failed to initialize config: {err}"));
    }
}

fn try_load_interactive_debug_mode(profile: Option<String>) -> bool {
    match config::init_session_config(profile) {
        Ok(()) => config::with_current_config("reading interactive debug setting", |cfg| cfg.settings.debug_mode),
        Err(err) => {
            log_warn!("Failed to initialize config for interactive startup: {}", err);
            false
        }
    }
}

fn load_runtime_config_settings() -> RuntimeConfigSettings {
    config::with_current_config("reading global settings", |cfg| RuntimeConfigSettings {
        debug_mode: cfg.settings.debug_mode,
        ssh_logging: cfg.settings.ssh_logging,
        show_title: cfg.settings.show_title,
    })
}

fn apply_debug_logging(logger: &log::Logger, args: &args::MainArgs, final_debug: log::DebugVerbosity, debug_from_config: bool) {
    if final_debug < log::DebugVerbosity::Safe {
        log_debug!("Debug mode not requested, disabling after initial config load");
        logger.disable_debug();
        return;
    }

    if logger.debug_verbosity() != final_debug {
        logger.enable_debug_with_verbosity(final_debug);
    }

    if final_debug >= log::DebugVerbosity::Raw {
        emit_raw_debug_warning_once();
    }

    match debug_mode_source(args, debug_from_config) {
        Some(DebugModeSource::CliRaw) => log_debug!("Raw debug mode enabled via CLI arguments"),
        Some(DebugModeSource::CliSafe) => log_debug!("Safe debug mode enabled via CLI argument"),
        Some(DebugModeSource::ConfigSafe) => log_debug!("Safe debug mode enabled via config file"),
        None => {}
    }
}

fn apply_ssh_logging(logger: &log::Logger, args: &args::MainArgs, ssh_logging_enabled: bool) {
    if !ssh_logging_enabled {
        return;
    }

    logger.enable_ssh_logging();
    if args.ssh_logging {
        log_info!("SSH logging enabled via CLI argument");
    } else {
        log_info!("SSH logging enabled via config file");
    }
}

fn print_title_banner(show_title: bool) {
    if !show_title {
        return;
    }

    log_debug!("Banner display enabled in config, printing banner");
    for line in TITLE_BANNER {
        println!("{line}\x1b[0m");
    }
}

fn update_session_name_for_logging(ssh_args: &[String]) {
    let session_hostname = ssh_args::extract_destination_host(ssh_args).unwrap_or_else(|| "unknown".to_string());

    // Use COSSH_SESSION_NAME when set by the session manager, otherwise fall back to the SSH host.
    let session_name = std::env::var("COSSH_SESSION_NAME").unwrap_or(session_hostname);
    let session_name = log::sanitize_session_name(&session_name);

    config::with_current_config_mut("setting session name", |cfg| {
        cfg.metadata.session_name = session_name.clone();
    });

    log_debug!("Session name set to: {session_name}");
}

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

/// Execute the `cossh` runtime and return the desired process exit code.
pub fn run() -> Result<ExitCode> {
    if auth::transport::is_internal_askpass_invocation() {
        return Ok(auth::run_internal_askpass());
    }

    let args = args::main_args();
    let logger = log::Logger::new();

    // Enable debug logging only when explicitly requested on the CLI. Config-based
    // debug mode is applied after the configuration has been loaded.
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
    let exit_code = process::process_handler(args.ssh_args, args.is_non_interactive, args.pass_entry).map_err(|err| {
        log_error!("Process handler failed: {}", err);
        eprintln!("Process failed: {err}");
        flush_debug_logs(&logger);
        err
    })?;

    log_info!("color-ssh exiting with code: {:?}", exit_code);
    flush_debug_logs(&logger);
    Ok(exit_code)
}

#[cfg(test)]
#[path = "test/runtime.rs"]
mod tests;
