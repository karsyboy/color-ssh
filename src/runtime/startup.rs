use crate::{config, log, log_debug, log_error, log_warn};

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

#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeConfigSettings {
    pub(crate) debug_mode: bool,
    pub(crate) ssh_logging: bool,
    pub(crate) show_title: bool,
}

pub(crate) fn exit_with_logged_error(logger: &log::Logger, message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    crate::runtime::logging::flush_debug_logs(logger);
    std::process::exit(1);
}

pub(crate) fn initialize_config_or_exit(logger: &log::Logger, profile: Option<String>, context: &str) {
    if let Err(err) = config::init_session_config(profile) {
        log_error!("{context}: {}", err);
        exit_with_logged_error(logger, format!("Failed to initialize config: {err}"));
    }
}

pub(crate) fn try_load_interactive_debug_mode(profile: Option<String>) -> bool {
    match config::init_session_config(profile) {
        Ok(()) => config::with_current_config("reading interactive debug setting", |cfg| cfg.settings.debug_mode),
        Err(err) => {
            log_warn!("Failed to initialize config for interactive startup: {}", err);
            false
        }
    }
}

pub(crate) fn load_runtime_config_settings() -> RuntimeConfigSettings {
    config::with_current_config("reading global settings", |cfg| RuntimeConfigSettings {
        debug_mode: cfg.settings.debug_mode,
        ssh_logging: cfg.settings.ssh_logging,
        show_title: cfg.settings.show_title,
    })
}

pub(crate) fn print_title_banner(show_title: bool) {
    if !show_title {
        return;
    }

    log_debug!("Banner display enabled in config, printing banner");
    for line in TITLE_BANNER {
        println!("{line}\x1b[0m");
    }
}
