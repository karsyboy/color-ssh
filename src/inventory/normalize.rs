use super::error::{InventoryResult, invalid_inventory};
use super::model::{InventoryHost, InventoryHostRaw, RdpHostOptions, SshHostOptions};
use super::path::expand_tilde;
use crate::validation::validate_vault_entry_name;
use std::path::Path;

pub(super) fn normalize_inventory_host(raw: InventoryHostRaw, source_file: &Path, folder_path: &[String]) -> InventoryResult<InventoryHost> {
    let host = raw
        .host
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_inventory(source_file, format!("inventory host '{}' is missing required field 'host'", raw.name)))?;

    if let Some(vault_pass) = raw.vault_pass.as_deref()
        && !validate_vault_entry_name(vault_pass)
    {
        return Err(invalid_inventory(
            source_file,
            format!("inventory host '{}' has invalid vault_pass '{}'", raw.name, vault_pass),
        ));
    }

    Ok(InventoryHost {
        name: raw.name,
        description: raw.description,
        protocol: raw.protocol,
        host,
        user: raw.user,
        port: raw.port,
        profile: raw.profile,
        vault_pass: raw.vault_pass,
        hidden: raw.hidden,
        ssh: SshHostOptions {
            identity_files: raw.identity_files.into_iter().map(|value| expand_tilde(&value)).collect(),
            identities_only: raw.identities_only,
            proxy_jump: raw.proxy_jump,
            proxy_command: raw.proxy_command,
            forward_agent: raw.forward_agent,
            local_forward: raw
                .local_forward
                .into_iter()
                .map(|value| crate::inventory::normalize_ssh_forward_spec(&value))
                .collect(),
            remote_forward: raw
                .remote_forward
                .into_iter()
                .map(|value| crate::inventory::normalize_ssh_forward_spec(&value))
                .collect(),
            extra_options: raw.ssh_options,
        },
        rdp: RdpHostOptions {
            domain: raw.rdp_domain,
            args: raw.rdp_args,
        },
        source_file: source_file.to_path_buf(),
        source_folder_path: folder_path.to_vec(),
    })
}
