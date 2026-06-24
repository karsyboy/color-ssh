//! Shared validation helpers for user-facing identifiers.

const INVALID_PROFILE_NAME_MESSAGE: &str = "invalid profile name: use only letters, numbers, '_' or '-'";
const INVALID_VAULT_ENTRY_NAME_MESSAGE: &str = "invalid pass entry name: use only letters, numbers, '.', '_' or '-'";

pub(crate) fn validate_profile_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

pub(crate) fn parse_profile_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if !validate_profile_name(trimmed) {
        return Err(INVALID_PROFILE_NAME_MESSAGE.to_string());
    }
    Ok(trimmed.to_string())
}

pub(crate) fn validate_vault_entry_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

pub(crate) fn parse_vault_entry_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if !validate_vault_entry_name(trimmed) {
        return Err(INVALID_VAULT_ENTRY_NAME_MESSAGE.to_string());
    }
    Ok(trimmed.to_string())
}
