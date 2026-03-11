use super::map_exit_code;
use std::process::ExitCode;

#[test]
fn map_exit_code_success_failure_and_missing_status_maps_to_expected_code() {
    let cases = [
        ((true, Some(0)), ExitCode::SUCCESS),
        ((false, Some(23)), ExitCode::from(23)),
        ((false, Some(300)), ExitCode::from(255)),
        ((false, None), ExitCode::from(1)),
    ];

    for ((is_success, status), expected) in cases {
        assert_eq!(map_exit_code(is_success, status), expected);
    }
}
