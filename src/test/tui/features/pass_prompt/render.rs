use super::{format_vault_time_left, format_vault_timeout_at};
use chrono::{Local, TimeZone};

#[test]
fn formats_short_vault_time_left_as_minutes_and_seconds() {
    assert_eq!(format_vault_time_left(Some(125)), "2:05");
}

#[test]
fn formats_long_vault_time_left_as_hours_minutes_and_seconds() {
    assert_eq!(format_vault_time_left(Some(3_725)), "1:02:05");
}

#[test]
fn formats_missing_vault_time_left_as_na() {
    assert_eq!(format_vault_time_left(None), "n/a");
}

#[test]
fn formats_vault_timeout_at_as_local_day_and_time() {
    let expected = Local
        .timestamp_opt(1_700_000_000, 0)
        .single()
        .expect("valid local timestamp")
        .format("%a %m-%d-%Y %I:%M:%S %p")
        .to_string();
    assert_eq!(format_vault_timeout_at(Some(1_700_000_000)), expected);
}

#[test]
fn formats_missing_vault_timeout_at_as_na() {
    assert_eq!(format_vault_timeout_at(None), "n/a");
}
