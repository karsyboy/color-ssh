use super::{DebugModeSource, debug_mode_source, resolve_logging_settings};
use crate::args::{MainArgs, MainCommand, ProtocolCommand, SshCommandArgs};
use crate::log::DebugVerbosity;

#[test]
fn test_mode_uses_only_cli_logging_flags() {
    let args = base_args(0, false, true);
    assert_eq!(resolve_logging_settings(&args, true, true), (DebugVerbosity::Off, false));

    let args = base_args(1, false, true);
    assert_eq!(resolve_logging_settings(&args, false, true), (DebugVerbosity::Safe, false));

    let args = base_args(2, true, true);
    assert_eq!(resolve_logging_settings(&args, true, false), (DebugVerbosity::Raw, true));
}

#[test]
fn normal_mode_merges_cli_and_config_logging_flags() {
    let args = base_args(0, false, false);
    assert_eq!(resolve_logging_settings(&args, true, true), (DebugVerbosity::Safe, true));

    let args = base_args(1, false, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Safe, false));

    let args = base_args(2, true, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Raw, true));

    let args = base_args(0, false, false);
    assert_eq!(resolve_logging_settings(&args, false, false), (DebugVerbosity::Off, false));
}

#[test]
fn debug_mode_source_prefers_cli_modes() {
    let args = base_args(2, false, false);
    assert_eq!(debug_mode_source(&args, true), Some(DebugModeSource::CliRaw));

    let args = base_args(1, false, false);
    assert_eq!(debug_mode_source(&args, true), Some(DebugModeSource::CliSafe));
}

#[test]
fn debug_mode_source_uses_config_only_without_cli_debug() {
    let args = base_args(0, false, false);
    assert_eq!(debug_mode_source(&args, true), Some(DebugModeSource::ConfigSafe));
    assert_eq!(debug_mode_source(&args, false), None);
}

fn base_args(debug_count: u8, ssh_logging: bool, test_mode: bool) -> MainArgs {
    MainArgs {
        debug_count,
        ssh_logging,
        test_mode,
        profile: None,
        interactive: false,
        pass_entry: None,
        command: Some(MainCommand::Protocol(ProtocolCommand::Ssh(SshCommandArgs {
            ssh_args: vec!["localhost".to_string()],
            is_non_interactive: false,
        }))),
    }
}
