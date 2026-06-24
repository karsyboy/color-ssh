use super::{DebugModeSource, debug_mode_source, resolve_logging_settings, resolve_session_name_for_logging};
use crate::args::{MainArgs, MainCommand, ProtocolCommand, SshCommandArgs};
use crate::log::DebugVerbosity;

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

#[test]
fn resolve_logging_settings_matches_safe_and_raw_modes() {
    assert_eq!(resolve_logging_settings(&base_args(1, false, true), false, true), (DebugVerbosity::Safe, false));
    assert_eq!(resolve_logging_settings(&base_args(2, true, false), false, false), (DebugVerbosity::Raw, true));
}

#[test]
fn debug_mode_source_prefers_cli_then_config() {
    assert_eq!(debug_mode_source(&base_args(2, false, false), true), Some(DebugModeSource::CliRaw));
    assert_eq!(debug_mode_source(&base_args(0, false, false), true), Some(DebugModeSource::ConfigSafe));
}

#[test]
fn resolve_session_name_for_logging_prefers_target_then_ssh_args_and_sanitizes() {
    assert_eq!(resolve_session_name_for_logging(Some("desktop01"), &[]), "desktop01");
    assert_eq!(resolve_session_name_for_logging(Some("bad/name"), &[]), "bad_name");
    assert_eq!(resolve_session_name_for_logging(None, &["admin@router01".to_string()]), "router01");
    assert_eq!(resolve_session_name_for_logging(None, &[]), "unknown");
}
