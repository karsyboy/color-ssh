//! Compatibility facade for process command preparation and spawning.

pub(crate) use super::command_spec::build_plain_ssh_command;
pub(crate) use super::rdp_builder::{build_rdp_command, build_rdp_command_for_host};
pub(crate) use super::spawn::{spawn_command, spawn_passthrough};
pub(crate) use super::ssh_builder::{build_ssh_command, resolve_host_by_destination};

#[cfg(test)]
pub(crate) use super::ssh_builder::{resolve_pass_entry_from_hosts, synthesize_ssh_args};
