//! Logging policy helpers used by runtime dispatch.

use crate::{args, config, log, log_debug, log_info, log_warn, ssh_args};
use std::sync::Once;

pub(crate) const APP_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Source that enabled debug mode for the current run.
pub(crate) enum DebugModeSource {
    CliSafe,
    CliRaw,
    ConfigSafe,
}

/// Resolve final debug and SSH logging settings after CLI/config precedence.
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

/// Determine which source enabled debug mode.
pub(crate) fn debug_mode_source(args: &args::MainArgs, debug_from_config: bool) -> Option<DebugModeSource> {
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

pub(crate) fn flush_debug_logs(logger: &log::Logger) {
    let _ = logger.flush_debug();
}

/// Apply resolved debug logging state to global logger.
pub(crate) fn apply_debug_logging(logger: &log::Logger, args: &args::MainArgs, final_debug: log::DebugVerbosity, debug_from_config: bool) {
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

/// Apply resolved SSH session logging state to global logger.
pub(crate) fn apply_ssh_logging(logger: &log::Logger, args: &args::MainArgs, ssh_logging_enabled: bool) {
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

/// Update per-session log filename stem from command target or env override.
pub(crate) fn update_session_name_for_logging(explicit_target: Option<&str>, ssh_args: &[String]) {
    let session_hostname = explicit_target
        .map(str::to_string)
        .or_else(|| ssh_args::extract_destination_host(ssh_args))
        .unwrap_or_else(|| "unknown".to_string());

    let session_name = std::env::var("COSSH_SESSION_NAME").unwrap_or(session_hostname);
    let session_name = log::sanitize_session_name(&session_name);

    config::with_current_config_mut("setting session name", |cfg| {
        cfg.metadata.session_name = session_name.clone();
    });

    log_debug!("Session name set to: {session_name}");
}
