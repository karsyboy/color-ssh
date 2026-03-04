//! Legacy OpenSSH config migration into the YAML inventory format.

use super::path::get_default_inventory_path;
use crate::ssh_config::{SshHost, TreeFolder, get_default_ssh_config_path, parse_ssh_config_for_migration};
use chrono::Local;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct MigrationSummary {
    /// Inventory output file path.
    pub(crate) output_path: PathBuf,
    /// Backup path when an existing inventory was replaced.
    pub(crate) backup_path: Option<PathBuf>,
    /// Number of hosts written to output.
    pub(crate) hosts_written: usize,
    /// Number of wildcard aliases skipped.
    pub(crate) wildcard_aliases_skipped: usize,
    /// Number of unsupported OpenSSH `Match` blocks skipped.
    pub(crate) unsupported_blocks_skipped: usize,
}

/// Migrate `~/.ssh/config` into the default YAML inventory path.
pub(crate) fn migrate_default_ssh_config_to_inventory() -> io::Result<MigrationSummary> {
    let ssh_config_path =
        get_default_ssh_config_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory for ~/.ssh/config"))?;
    let inventory_path =
        get_default_inventory_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory for ~/.color-ssh"))?;

    migrate_ssh_config_to_inventory(&ssh_config_path, &inventory_path)
}

fn migrate_ssh_config_to_inventory(ssh_config_path: &Path, inventory_path: &Path) -> io::Result<MigrationSummary> {
    let parsed = parse_ssh_config_for_migration(ssh_config_path)?;
    let mut render_state = RenderState::default();
    let rendered = render_inventory_document(&parsed.root, &parsed.hosts, &mut render_state)?;

    let backup_path = backup_existing_inventory(inventory_path)?;
    if let Some(parent) = inventory_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(inventory_path, rendered)?;

    Ok(MigrationSummary {
        output_path: inventory_path.to_path_buf(),
        backup_path,
        hosts_written: render_state.hosts_written,
        wildcard_aliases_skipped: render_state.wildcard_aliases_skipped,
        unsupported_blocks_skipped: parsed.unsupported_blocks,
    })
}

fn backup_existing_inventory(inventory_path: &Path) -> io::Result<Option<PathBuf>> {
    if !inventory_path.exists() {
        return Ok(None);
    }

    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let backup_name = format!("cossh-inventory.yaml.bak-{timestamp}");
    let backup_path = inventory_path.with_file_name(backup_name);
    fs::copy(inventory_path, &backup_path)?;
    Ok(Some(backup_path))
}

#[derive(Default)]
struct RenderState {
    seen_names: HashSet<String>,
    hosts_written: usize,
    wildcard_aliases_skipped: usize,
}

fn render_inventory_document(root: &TreeFolder, hosts: &[SshHost], state: &mut RenderState) -> io::Result<String> {
    let items = render_root_items(root, hosts, state)?;
    if items.is_empty() {
        return Ok("inventory: []\n".to_string());
    }

    let mut output = String::from("inventory:\n\n");
    output.push_str(&items.join("\n\n"));
    output.push('\n');
    Ok(output)
}

fn render_root_items(root: &TreeFolder, hosts: &[SshHost], state: &mut RenderState) -> io::Result<Vec<String>> {
    let mut items = Vec::new();

    for &host_idx in &root.host_indices {
        if let Some(host) = hosts.get(host_idx)
            && let Some(rendered) = render_host_item(host, 2, state)?
        {
            items.push(rendered);
        }
    }

    for child in &root.children {
        if let Some(rendered) = render_folder_item(child, hosts, 2, state)? {
            items.push(rendered);
        }
    }

    Ok(items)
}

fn render_folder_item(folder: &TreeFolder, hosts: &[SshHost], indent: usize, state: &mut RenderState) -> io::Result<Option<String>> {
    let mut nested_items = Vec::new();

    for &host_idx in &folder.host_indices {
        if let Some(host) = hosts.get(host_idx)
            && let Some(rendered) = render_host_item(host, indent + 4, state)?
        {
            nested_items.push(rendered);
        }
    }

    for child in &folder.children {
        if let Some(rendered) = render_folder_item(child, hosts, indent + 4, state)? {
            nested_items.push(rendered);
        }
    }

    if nested_items.is_empty() {
        return Ok(None);
    }

    let indent_str = " ".repeat(indent);
    Ok(Some(format!(
        "{indent_str}- {}:\n{}",
        quote_yaml_string(&folder_display_name(folder)),
        nested_items.join("\n\n")
    )))
}

fn render_host_item(host: &SshHost, indent: usize, state: &mut RenderState) -> io::Result<Option<String>> {
    if host.name.contains('*') || host.name.contains('?') {
        state.wildcard_aliases_skipped += 1;
        return Ok(None);
    }

    if !state.seen_names.insert(host.name.clone()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("duplicate SSH alias '{}' found during migration", host.name),
        ));
    }

    state.hosts_written += 1;
    let indent_str = " ".repeat(indent);
    let field_indent = " ".repeat(indent + 2);
    let list_indent = " ".repeat(indent + 4);

    let mut lines = Vec::new();
    lines.push(format!("{indent_str}- name: {}", quote_yaml_string(&host.name)));
    if let Some(description) = host.description.as_ref() {
        lines.push(format!("{field_indent}description: {}", quote_yaml_string(description)));
    }

    lines.push(format!("{field_indent}protocol: {}", quote_yaml_string(host.protocol.as_str())));
    lines.push(format!(
        "{field_indent}host: {}",
        quote_yaml_string(host.hostname.as_deref().unwrap_or(&host.name))
    ));

    if let Some(user) = host.user.as_ref() {
        lines.push(format!("{field_indent}user: {}", quote_yaml_string(user)));
    }
    if let Some(port) = host.port {
        lines.push(format!("{field_indent}port: {port}"));
    }
    if let Some(profile) = host.profile.as_ref() {
        lines.push(format!("{field_indent}profile: {}", quote_yaml_string(profile)));
    }
    if let Some(vault_pass) = host.pass_key.as_ref() {
        lines.push(format!("{field_indent}vault_pass: {}", quote_yaml_string(vault_pass)));
    }
    if host.hidden {
        lines.push(format!("{field_indent}hidden: true"));
    }
    if let Some(identity_file) = host.identity_files.first() {
        if host.identity_files.len() == 1 {
            lines.push(format!("{field_indent}identity_file: {}", quote_yaml_string(identity_file)));
        } else {
            lines.push(format!("{field_indent}identity_file:"));
            for identity_file in &host.identity_files {
                lines.push(format!("{list_indent}- {}", quote_yaml_string(identity_file)));
            }
        }
    }
    if let Some(identities_only) = host.identities_only {
        lines.push(format!("{field_indent}identities_only: {identities_only}"));
    }
    if let Some(proxy_jump) = host.proxy_jump.as_ref() {
        lines.push(format!("{field_indent}proxy_jump: {}", quote_yaml_string(proxy_jump)));
    }
    if let Some(proxy_command) = host.proxy_command.as_ref() {
        lines.push(format!("{field_indent}proxy_command: {}", quote_yaml_string(proxy_command)));
    }
    if let Some(forward_agent) = host.forward_agent.as_ref() {
        lines.push(format!("{field_indent}forward_agent: {}", quote_yaml_string(forward_agent)));
    }
    if !host.local_forward.is_empty() {
        lines.push(format!("{field_indent}local_forward:"));
        for forward in &host.local_forward {
            lines.push(format!(
                "{list_indent}- {}",
                quote_yaml_string(&crate::inventory::normalize_ssh_forward_spec(forward))
            ));
        }
    }
    if !host.remote_forward.is_empty() {
        lines.push(format!("{field_indent}remote_forward:"));
        for forward in &host.remote_forward {
            lines.push(format!(
                "{list_indent}- {}",
                quote_yaml_string(&crate::inventory::normalize_ssh_forward_spec(forward))
            ));
        }
    }
    if let Some(rdp_domain) = host.rdp_domain.as_ref() {
        lines.push(format!("{field_indent}rdp_domain: {}", quote_yaml_string(rdp_domain)));
    }
    if !host.rdp_args.is_empty() {
        lines.push(format!("{field_indent}rdp_args:"));
        for arg in &host.rdp_args {
            lines.push(format!("{list_indent}- {}", quote_yaml_string(arg)));
        }
    }
    if !host.other_options.is_empty() {
        lines.push(format!("{field_indent}ssh_options:"));
        let mut keys: Vec<_> = host.other_options.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(values) = host.other_options.get(key) {
                if values.len() <= 1 {
                    if let Some(value) = values.first() {
                        lines.push(format!("{list_indent}{}: {}", quote_yaml_key(key), quote_yaml_string(value)));
                    }
                } else {
                    lines.push(format!("{list_indent}{}:", quote_yaml_key(key)));
                    let nested_indent = " ".repeat(indent + 6);
                    for value in values {
                        lines.push(format!("{nested_indent}- {}", quote_yaml_string(value)));
                    }
                }
            }
        }
    }

    Ok(Some(lines.join("\n")))
}

fn folder_display_name(folder: &TreeFolder) -> String {
    folder
        .path
        .file_stem()
        .or_else(|| folder.path.file_name())
        .and_then(|segment| segment.to_str())
        .unwrap_or(&folder.name)
        .to_string()
}

fn quote_yaml_key(value: &str) -> String {
    quote_yaml_string(value)
}

fn quote_yaml_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
#[path = "../test/inventory/migration.rs"]
mod tests;
