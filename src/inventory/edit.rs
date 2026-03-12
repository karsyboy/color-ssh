//! Inventory YAML mutation helpers for TUI host entry management.

use super::{ConnectionProtocol, SshOptionMap, normalize_ssh_forward_spec};
use serde_yml::{Mapping, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EditableInventoryHost {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) protocol: ConnectionProtocol,
    pub(crate) host: String,
    pub(crate) user: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) profile: Option<String>,
    pub(crate) vault_pass: Option<String>,
    pub(crate) hidden: bool,
    pub(crate) ssh_identity_files: Vec<String>,
    pub(crate) ssh_identities_only: Option<bool>,
    pub(crate) ssh_proxy_jump: Option<String>,
    pub(crate) ssh_proxy_command: Option<String>,
    pub(crate) ssh_forward_agent: Option<String>,
    pub(crate) ssh_local_forward: Vec<String>,
    pub(crate) ssh_remote_forward: Vec<String>,
    pub(crate) ssh_options: SshOptionMap,
    pub(crate) rdp_domain: Option<String>,
    pub(crate) rdp_args: Vec<String>,
}

pub(crate) fn update_inventory_host_entry(source_file: &Path, original_name: &str, host: &EditableInventoryHost) -> io::Result<()> {
    let mut document = load_inventory_document(source_file)?;
    let nodes = inventory_nodes_mut(&mut document, source_file)?;

    if !update_host_entry_in_nodes(nodes, original_name, host) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("inventory host '{}' was not found in '{}'", original_name, source_file.display()),
        ));
    }

    write_inventory_document(source_file, &document)
}

pub(crate) fn delete_inventory_host_entry(source_file: &Path, host_name: &str) -> io::Result<()> {
    let mut document = load_inventory_document(source_file)?;
    let nodes = inventory_nodes_mut(&mut document, source_file)?;

    if !delete_host_entry_in_nodes(nodes, host_name) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("inventory host '{}' was not found in '{}'", host_name, source_file.display()),
        ));
    }

    write_inventory_document(source_file, &document)
}

pub(crate) fn create_inventory_host_entry(source_file: &Path, folder_path: &[String], host: &EditableInventoryHost) -> io::Result<()> {
    let mut document = load_inventory_document(source_file)?;
    let nodes = inventory_nodes_mut(&mut document, source_file)?;
    let target_nodes = ensure_folder_nodes(nodes, folder_path, source_file)?;
    target_nodes.push(Value::Mapping(editable_host_mapping(host)));
    write_inventory_document(source_file, &document)
}

fn load_inventory_document(source_file: &Path) -> io::Result<Value> {
    if !source_file.exists() {
        return Ok(Value::Mapping(Mapping::new()));
    }

    let contents = fs::read_to_string(source_file)?;
    if contents.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }

    let parsed = serde_yml::from_str::<Value>(&contents).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse inventory YAML '{}': {err}", source_file.display()),
        )
    })?;

    match parsed {
        Value::Mapping(_) => Ok(parsed),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("inventory file root must be a mapping: '{}'", source_file.display()),
        )),
    }
}

fn write_inventory_document(source_file: &Path, document: &Value) -> io::Result<()> {
    let mut rendered = serde_yml::to_string(document).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize inventory YAML '{}': {err}", source_file.display()),
        )
    })?;

    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }

    if let Some(parent) = source_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = temporary_inventory_path(source_file);
    fs::write(&tmp_path, rendered)?;
    if let Err(err) = fs::rename(&tmp_path, source_file) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    Ok(())
}

fn temporary_inventory_path(source_file: &Path) -> PathBuf {
    let file_name = source_file.file_name().and_then(|segment| segment.to_str()).unwrap_or("cossh-inventory.yaml");
    let process_id = std::process::id();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    source_file.with_file_name(format!(".{file_name}.tmp-{process_id}-{nanos}"))
}

fn inventory_nodes_mut<'a>(document: &'a mut Value, source_file: &Path) -> io::Result<&'a mut Vec<Value>> {
    let Value::Mapping(root) = document else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("inventory file root must be a mapping: '{}'", source_file.display()),
        ));
    };

    let inventory_key = root.iter().find_map(|(key, _)| {
        value_string_key(key).and_then(|text| {
            if canonical_top_level_key(text) == "inventory" {
                Some(key.clone())
            } else {
                None
            }
        })
    });

    let key = inventory_key.unwrap_or_else(|| Value::String("inventory".to_string()));
    if !root.contains_key(&key) {
        root.insert(key.clone(), Value::Sequence(Vec::new()));
    }

    let value = root
        .get_mut(&key)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("inventory key is missing in '{}'", source_file.display())))?;

    if matches!(value, Value::Null) {
        *value = Value::Sequence(Vec::new());
    }

    match value {
        Value::Sequence(nodes) => Ok(nodes),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("inventory must be a YAML list in '{}'", source_file.display()),
        )),
    }
}

fn update_host_entry_in_nodes(nodes: &mut Vec<Value>, original_name: &str, host: &EditableInventoryHost) -> bool {
    for node in nodes {
        {
            if let Some(mapping) = host_mapping_mut(node)
                && host_name_matches(mapping, original_name)
            {
                apply_editable_host_to_mapping(mapping, host);
                return true;
            }
        }

        if let Some(children) = folder_items_mut(node)
            && update_host_entry_in_nodes(children, original_name, host)
        {
            return true;
        }
    }

    false
}

fn delete_host_entry_in_nodes(nodes: &mut Vec<Value>, host_name: &str) -> bool {
    let mut index = 0usize;

    while index < nodes.len() {
        let mut should_remove = false;
        let mut removed_from_child = false;

        {
            let node = &mut nodes[index];
            if let Some(mapping) = host_mapping_mut(node) {
                should_remove = host_name_matches(mapping, host_name);
            } else if let Some(children) = folder_items_mut(node) {
                removed_from_child = delete_host_entry_in_nodes(children, host_name);
            }
        }

        if should_remove {
            nodes.remove(index);
            return true;
        }

        if removed_from_child {
            return true;
        }

        index += 1;
    }

    false
}

fn ensure_folder_nodes<'a>(nodes: &'a mut Vec<Value>, folder_path: &[String], source_file: &Path) -> io::Result<&'a mut Vec<Value>> {
    let mut current = nodes;

    for segment in folder_path {
        if segment.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("folder path segment cannot be empty in '{}'", source_file.display()),
            ));
        }

        let existing_index = current.iter().position(|node| folder_entry_name(node).is_some_and(|name| name == segment));

        let index = if let Some(index) = existing_index {
            index
        } else {
            let mut folder_mapping = Mapping::new();
            folder_mapping.insert(Value::String(segment.clone()), Value::Sequence(Vec::new()));
            current.push(Value::Mapping(folder_mapping));
            current.len().saturating_sub(1)
        };

        let Some(next) = folder_items_mut(&mut current[index]) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("folder '{}' in '{}' must contain a YAML list", segment, source_file.display()),
            ));
        };

        current = next;
    }

    Ok(current)
}

fn apply_editable_host_to_mapping(mapping: &mut Mapping, host: &EditableInventoryHost) {
    remove_editable_host_keys(mapping);
    for (key, value) in editable_host_mapping(host) {
        mapping.insert(key, value);
    }
}

fn remove_editable_host_keys(mapping: &mut Mapping) {
    let keys_to_remove = mapping
        .keys()
        .filter_map(|key| {
            let key_text = value_string_key(key)?;
            editable_host_key(canonical_host_key(key_text)).then(|| key.clone())
        })
        .collect::<Vec<_>>();

    for key in keys_to_remove {
        let _ = mapping.remove(&key);
    }
}

fn editable_host_key(key: &str) -> bool {
    matches!(
        key,
        "name"
            | "description"
            | "protocol"
            | "host"
            | "user"
            | "port"
            | "profile"
            | "vault_pass"
            | "hidden"
            | "identity_file"
            | "identities_only"
            | "proxy_jump"
            | "proxy_command"
            | "forward_agent"
            | "local_forward"
            | "remote_forward"
            | "ssh_options"
            | "rdp_domain"
            | "rdp_args"
    )
}

fn editable_host_mapping(host: &EditableInventoryHost) -> Mapping {
    let mut mapping = Mapping::new();

    mapping.insert(Value::String("name".to_string()), Value::String(host.name.clone()));
    mapping.insert(Value::String("protocol".to_string()), Value::String(host.protocol.as_str().to_string()));
    mapping.insert(Value::String("host".to_string()), Value::String(host.host.clone()));
    mapping.insert(Value::String("hidden".to_string()), Value::Bool(host.hidden));

    if let Some(description) = trimmed_option(&host.description) {
        mapping.insert(Value::String("description".to_string()), Value::String(description.to_string()));
    }
    if let Some(user) = trimmed_option(&host.user) {
        mapping.insert(Value::String("user".to_string()), Value::String(user.to_string()));
    }
    if let Some(port) = host.port {
        mapping.insert(Value::String("port".to_string()), Value::Number(port.into()));
    }
    if let Some(profile) = trimmed_option(&host.profile) {
        mapping.insert(Value::String("profile".to_string()), Value::String(profile.to_string()));
    }
    if let Some(vault_pass) = trimmed_option(&host.vault_pass) {
        mapping.insert(Value::String("vault_pass".to_string()), Value::String(vault_pass.to_string()));
    }

    if !host.ssh_identity_files.is_empty() {
        mapping.insert(
            Value::String("identity_file".to_string()),
            Value::Sequence(host.ssh_identity_files.iter().map(|value| Value::String(value.clone())).collect()),
        );
    }

    if let Some(identities_only) = host.ssh_identities_only {
        mapping.insert(Value::String("identities_only".to_string()), Value::Bool(identities_only));
    }

    if let Some(proxy_jump) = trimmed_option(&host.ssh_proxy_jump) {
        mapping.insert(Value::String("proxy_jump".to_string()), Value::String(proxy_jump.to_string()));
    }

    if let Some(proxy_command) = trimmed_option(&host.ssh_proxy_command) {
        mapping.insert(Value::String("proxy_command".to_string()), Value::String(proxy_command.to_string()));
    }

    if let Some(forward_agent) = trimmed_option(&host.ssh_forward_agent) {
        mapping.insert(Value::String("forward_agent".to_string()), Value::String(forward_agent.to_string()));
    }

    let normalized_local_forward = host.ssh_local_forward.iter().map(|value| normalize_ssh_forward_spec(value)).collect::<Vec<_>>();
    if !normalized_local_forward.is_empty() {
        mapping.insert(
            Value::String("local_forward".to_string()),
            Value::Sequence(normalized_local_forward.iter().map(|value| Value::String(value.clone())).collect()),
        );
    }

    let normalized_remote_forward = host
        .ssh_remote_forward
        .iter()
        .map(|value| normalize_ssh_forward_spec(value))
        .collect::<Vec<_>>();
    if !normalized_remote_forward.is_empty() {
        mapping.insert(
            Value::String("remote_forward".to_string()),
            Value::Sequence(normalized_remote_forward.iter().map(|value| Value::String(value.clone())).collect()),
        );
    }

    if !host.ssh_options.is_empty() {
        let mut ssh_options = Mapping::new();
        for (key, values) in &host.ssh_options {
            if values.is_empty() {
                continue;
            }

            let value = if values.len() == 1 {
                Value::String(values[0].clone())
            } else {
                Value::Sequence(values.iter().map(|item| Value::String(item.clone())).collect())
            };

            ssh_options.insert(Value::String(key.clone()), value);
        }

        if !ssh_options.is_empty() {
            mapping.insert(Value::String("ssh_options".to_string()), Value::Mapping(ssh_options));
        }
    }

    if let Some(rdp_domain) = trimmed_option(&host.rdp_domain) {
        mapping.insert(Value::String("rdp_domain".to_string()), Value::String(rdp_domain.to_string()));
    }

    if !host.rdp_args.is_empty() {
        mapping.insert(
            Value::String("rdp_args".to_string()),
            Value::Sequence(host.rdp_args.iter().map(|value| Value::String(value.clone())).collect()),
        );
    }

    mapping
}

fn host_name_matches(mapping: &Mapping, target_name: &str) -> bool {
    mapping.iter().any(|(key, value)| {
        let Some(key_text) = value_string_key(key) else {
            return false;
        };
        if canonical_host_key(key_text) != "name" {
            return false;
        }

        scalar_value_to_string(value).map(|name| name == target_name).unwrap_or(false)
    })
}

fn host_mapping_mut(value: &mut Value) -> Option<&mut Mapping> {
    let Value::Mapping(mapping) = value else {
        return None;
    };
    if is_host_mapping(mapping) { Some(mapping) } else { None }
}

fn is_host_mapping(mapping: &Mapping) -> bool {
    mapping.iter().any(|(key, _)| {
        value_string_key(key)
            .map(canonical_host_key)
            .is_some_and(|canonical_key| canonical_key == "name")
    })
}

fn folder_entry_name(value: &Value) -> Option<&str> {
    let Value::Mapping(mapping) = value else {
        return None;
    };
    if is_host_mapping(mapping) || mapping.len() != 1 {
        return None;
    }

    let (key, _) = mapping.iter().next()?;
    value_string_key(key)
}

fn folder_items_mut(value: &mut Value) -> Option<&mut Vec<Value>> {
    let Value::Mapping(mapping) = value else {
        return None;
    };
    if is_host_mapping(mapping) || mapping.len() != 1 {
        return None;
    }

    let (_, items) = mapping.iter_mut().next()?;
    let Value::Sequence(items) = items else {
        return None;
    };

    Some(items)
}

fn value_string_key(value: &Value) -> Option<&str> {
    let Value::String(text) = value else {
        return None;
    };

    Some(text.as_str())
}

fn scalar_value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn trimmed_option(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|candidate| !candidate.is_empty())
}

fn canonical_top_level_key(key: &str) -> &str {
    match compact_key(key).as_str() {
        "include" => "include",
        "inventory" => "inventory",
        _ => key,
    }
}

fn canonical_host_key(key: &str) -> &str {
    match compact_key(key).as_str() {
        "name" => "name",
        "description" => "description",
        "protocol" => "protocol",
        "host" | "hostname" => "host",
        "user" => "user",
        "port" => "port",
        "profile" => "profile",
        "vaultpass" => "vault_pass",
        "hidden" => "hidden",
        "identityfile" => "identity_file",
        "identitiesonly" => "identities_only",
        "proxyjump" => "proxy_jump",
        "proxycommand" => "proxy_command",
        "forwardagent" => "forward_agent",
        "localforward" => "local_forward",
        "remoteforward" => "remote_forward",
        "sshoptions" => "ssh_options",
        "rdpdomain" => "rdp_domain",
        "rdpargs" => "rdp_args",
        _ => key,
    }
}

fn compact_key(key: &str) -> String {
    key.chars().filter(|ch| ch.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
#[path = "../test/inventory/edit.rs"]
mod tests;
