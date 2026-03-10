//! Compatibility facade for process command preparation and spawning.

pub(crate) use super::command_spec::build_plain_ssh_command;
pub(crate) use super::rdp_builder::{build_rdp_command, build_rdp_command_for_host_with_auth_settings};
pub(crate) use super::spawn::spawn_passthrough;
pub(crate) use super::ssh_builder::{build_ssh_command, build_ssh_command_for_host, resolve_host_by_destination};

#[cfg(test)]
pub(crate) use super::ssh_builder::{resolve_pass_entry_from_hosts, synthesize_ssh_args};
