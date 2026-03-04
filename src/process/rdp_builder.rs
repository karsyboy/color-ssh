//! RDP command construction and inventory/default resolution.

use super::command_spec::PreparedCommand;
use super::ssh_builder::resolve_host_by_destination;
use super::vault::resolve_vault_password;
use crate::args::RdpCommandArgs;
use crate::auth::secret::{ExposeSecret, SensitiveString};
use crate::inventory::{ConnectionProtocol, InventoryHost};
use std::io;

fn rdp_server_address(host: &InventoryHost) -> String {
    let destination = host.host.as_str();
    match host.port {
        Some(port) if destination.contains(':') && !destination.starts_with('[') => format!("[{destination}]:{port}"),
        Some(port) => format!("{destination}:{port}"),
        None => destination.to_string(),
    }
}

fn has_rdp_cert_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg.starts_with("/cert:") || arg == "/cert")
}

fn build_rdp_stdin_payload(host: &InventoryHost, password: SensitiveString) -> io::Result<SensitiveString> {
    let Some(user) = host.user.as_deref().filter(|value| !value.trim().is_empty()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP username is required; set `user` in the inventory or pass `--user`",
        ));
    };

    let server = rdp_server_address(host);
    let mut args = Vec::with_capacity(host.rdp.args.len() + 6);
    args.push(format!("/u:{user}"));
    if let Some(domain) = host.rdp.domain.as_deref().filter(|value| !value.trim().is_empty()) {
        args.push(format!("/d:{domain}"));
    }
    args.push(format!("/v:{server}"));
    args.push(format!("/p:{}", password.expose_secret()));
    // Keep default cert handling explicit when caller did not specify one.
    if !has_rdp_cert_flag(&host.rdp.args) {
        args.push("/cert:tofu".to_string());
    }
    args.extend(host.rdp.args.iter().cloned());

    Ok(SensitiveString::from(args.join("\n") + "\n"))
}

pub(crate) fn build_rdp_command_for_host(host: &InventoryHost, explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let pass_entry_name = explicit_pass_entry.map(str::to_string).or_else(|| host.vault_pass.clone()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "RDP launch requires a password vault entry; set `vault_pass` in the inventory or pass `--pass-entry`",
        )
    })?;

    let password = resolve_vault_password(&pass_entry_name)?;
    let stdin_payload = build_rdp_stdin_payload(host, password)?;
    let mut command = PreparedCommand::new("xfreerdp", vec!["/args-from:stdin".to_string()]);
    command.stdin_payload = Some(stdin_payload);
    Ok(command)
}

pub(crate) fn build_rdp_command(args: &RdpCommandArgs, explicit_pass_entry: Option<&str>) -> io::Result<PreparedCommand> {
    let configured_host = crate::inventory::load_inventory_tree()
        .ok()
        .and_then(|tree| resolve_host_by_destination(&args.target, &tree.hosts).cloned());

    let mut host = configured_host.unwrap_or_else(|| {
        let mut host = InventoryHost::new(args.target.clone());
        host.host = args.target.clone();
        host
    });

    host.protocol = ConnectionProtocol::Rdp;
    if let Some(user) = args.user.as_ref() {
        host.user = Some(user.clone());
    }
    if let Some(domain) = args.domain.as_ref() {
        host.rdp.domain = Some(domain.clone());
    }
    if let Some(port) = args.port {
        host.port = Some(port);
    }
    host.rdp.args.extend(args.extra_args.iter().cloned());

    build_rdp_command_for_host(&host, explicit_pass_entry)
}
