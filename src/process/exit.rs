use std::process::ExitCode;

pub(super) fn map_exit_code(success: bool, code: Option<i32>) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        let clamped_code = code.map_or(1, |status_code| u8::try_from(status_code).unwrap_or(255));
        ExitCode::from(clamped_code)
    }
}
