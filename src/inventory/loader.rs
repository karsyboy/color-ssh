//! Inventory parsing, normalization, and include merging.

use super::include::{expand_include_pattern, resolve_include_pattern};
use super::model::{FolderId, InventoryDocumentRaw, InventoryHost, InventoryHostRaw, InventoryNodeRaw, InventoryTreeModel, TreeFolder};
use super::path::expand_tilde;
use crate::log_debug;
use crate::validation::validate_vault_entry_name;
use serde_yml::{Mapping, Value};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct FolderAccumulator {
    name: String,
    path: PathBuf,
    children: Vec<FolderAccumulator>,
    host_indices: Vec<usize>,
}

impl FolderAccumulator {
    fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            children: Vec::new(),
            host_indices: Vec::new(),
        }
    }

    fn child_mut(&mut self, name: &str, path: &Path) -> &mut FolderAccumulator {
        if let Some(index) = self.children.iter().position(|child| child.name == name) {
            return &mut self.children[index];
        }

        self.children.push(FolderAccumulator::new(name.to_string(), path.to_path_buf()));
        let index = self.children.len().saturating_sub(1);
        &mut self.children[index]
    }
}

pub(super) fn build_inventory_tree(inventory_path: &Path) -> io::Result<InventoryTreeModel> {
    let root_name = inventory_path
        .file_name()
        .and_then(|segment| segment.to_str())
        .unwrap_or("cossh-inventory.yaml")
        .to_string();
    let mut root = FolderAccumulator::new(root_name, inventory_path.to_path_buf());
    let mut hosts = Vec::new();
    let mut seen_host_names = HashMap::new();
    let mut visited = std::collections::HashSet::new();

    load_document_recursive(inventory_path, &mut root, &mut hosts, &mut seen_host_names, &mut visited)?;

    let mut next_id: FolderId = 0;
    let mut tree_root = finalize_folder(root, &mut next_id);
    sort_tree_folder(&mut tree_root, &hosts);
    Ok(InventoryTreeModel { root: tree_root, hosts })
}

fn load_document_recursive(
    inventory_path: &Path,
    root: &mut FolderAccumulator,
    hosts: &mut Vec<InventoryHost>,
    seen_host_names: &mut HashMap<String, PathBuf>,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> io::Result<()> {
    let canonical = inventory_path.canonicalize().unwrap_or_else(|_| inventory_path.to_path_buf());

    if !visited.insert(canonical.clone()) {
        log_debug!("Skipping already visited inventory file (possible include cycle): {}", canonical.display());
        return Ok(());
    }

    let parsed = parse_inventory_document(&canonical)?;
    let parent_dir = canonical.parent().unwrap_or(Path::new("."));

    for include in parsed.include {
        let resolved_pattern = resolve_include_pattern(&include, parent_dir);
        for include_path in expand_include_pattern(&resolved_pattern) {
            load_document_recursive(&include_path, root, hosts, seen_host_names, visited)?;
        }
    }

    for node in parsed.inventory {
        add_inventory_node(node, root, hosts, seen_host_names, &canonical, &[])?;
    }

    Ok(())
}

fn finalize_folder(folder: FolderAccumulator, next_id: &mut FolderId) -> TreeFolder {
    let folder_id = *next_id;
    *next_id += 1;

    TreeFolder {
        id: folder_id,
        name: folder.name,
        path: folder.path,
        children: folder.children.into_iter().map(|child| finalize_folder(child, next_id)).collect(),
        host_indices: folder.host_indices,
    }
}

fn sort_tree_folder(folder: &mut TreeFolder, hosts: &[InventoryHost]) {
    folder.host_indices.sort_by(|left_idx, right_idx| {
        let left_name = hosts.get(*left_idx).map(|host| host.name.as_str()).unwrap_or_default();
        let right_name = hosts.get(*right_idx).map(|host| host.name.as_str()).unwrap_or_default();
        let left_key = left_name.to_ascii_lowercase();
        let right_key = right_name.to_ascii_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left_name.cmp(right_name))
            .then_with(|| left_idx.cmp(right_idx))
    });

    for child in &mut folder.children {
        sort_tree_folder(child, hosts);
    }

    folder.children.sort_by(|left, right| {
        let left_key = left.name.to_ascii_lowercase();
        let right_key = right.name.to_ascii_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn add_inventory_node(
    node: InventoryNodeRaw,
    folder: &mut FolderAccumulator,
    hosts: &mut Vec<InventoryHost>,
    seen_host_names: &mut HashMap<String, PathBuf>,
    source_file: &Path,
    folder_path: &[String],
) -> io::Result<()> {
    match node {
        InventoryNodeRaw::Host(raw) => {
            let host = normalize_inventory_host(raw, source_file, folder_path)?;
            if let Some(previous_path) = seen_host_names.insert(host.name.clone(), host.source_file.clone()) {
                return Err(invalid_inventory(
                    source_file,
                    format!(
                        "duplicate inventory host '{}' found in '{}' and '{}'",
                        host.name,
                        previous_path.display(),
                        host.source_file.display()
                    ),
                ));
            }

            folder.host_indices.push(hosts.len());
            hosts.push(host);
            Ok(())
        }
        InventoryNodeRaw::Folder { name, items } => {
            let child = folder.child_mut(&name, source_file);
            let mut child_path = folder_path.to_vec();
            child_path.push(name);
            for item in items {
                add_inventory_node(item, child, hosts, seen_host_names, source_file, &child_path)?;
            }
            Ok(())
        }
    }
}

fn normalize_inventory_host(raw: InventoryHostRaw, source_file: &Path, folder_path: &[String]) -> io::Result<InventoryHost> {
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
        ssh: crate::inventory::SshHostOptions {
            identity_file: raw.identity_file.map(|value| expand_tilde(&value)),
            identities_only: raw.identities_only,
            proxy_jump: raw.proxy_jump,
            proxy_command: raw.proxy_command,
            forward_agent: raw.forward_agent,
            local_forward: raw.local_forward,
            remote_forward: raw.remote_forward,
            extra_options: raw.ssh_options,
        },
        rdp: crate::inventory::RdpHostOptions {
            domain: raw.rdp_domain,
            args: raw.rdp_args,
        },
        source_file: source_file.to_path_buf(),
        source_folder_path: folder_path.to_vec(),
    })
}

fn parse_inventory_document(inventory_path: &Path) -> io::Result<InventoryDocumentRaw> {
    let contents = fs::read_to_string(inventory_path)?;
    let yaml: Value = serde_yml::from_str(&contents)
        .map_err(|err| invalid_inventory(inventory_path, format!("failed to parse inventory YAML '{}': {err}", inventory_path.display())))?;
    parse_inventory_document_value(&yaml, inventory_path)
}

fn parse_inventory_document_value(yaml: &Value, source_file: &Path) -> io::Result<InventoryDocumentRaw> {
    let Value::Mapping(mapping) = yaml else {
        return Err(invalid_inventory(source_file, "inventory file root must be a mapping"));
    };

    let include = match mapping_value(mapping, "include") {
        Some(value) => parse_string_list(value, source_file, "include", false)?,
        None => Vec::new(),
    };
    let inventory = match mapping_value(mapping, "inventory") {
        Some(value) => parse_inventory_nodes(value, source_file)?,
        None => Vec::new(),
    };

    Ok(InventoryDocumentRaw { include, inventory })
}

fn parse_inventory_nodes(value: &Value, source_file: &Path) -> io::Result<Vec<InventoryNodeRaw>> {
    let Value::Sequence(sequence) = value else {
        return Err(invalid_inventory(source_file, "inventory must be a YAML list"));
    };

    sequence.iter().map(|item| parse_inventory_node(item, source_file)).collect()
}

fn parse_inventory_node(value: &Value, source_file: &Path) -> io::Result<InventoryNodeRaw> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid_inventory(
            source_file,
            "inventory entries must be mappings containing a host definition or folder item",
        ));
    };

    if mapping_has_key(mapping, "name") {
        return Ok(InventoryNodeRaw::Host(parse_inventory_host(mapping, source_file)?));
    }

    if mapping.len() != 1 {
        return Err(invalid_inventory(
            source_file,
            "folder entries must contain exactly one key of the form '- FolderName: [...]'",
        ));
    }

    let (folder_name, folder_items) = mapping
        .iter()
        .next()
        .ok_or_else(|| invalid_inventory(source_file, "folder entry cannot be empty"))?;
    let folder_name = scalar_to_string(folder_name, source_file, "folder name")?;
    let items = parse_inventory_nodes(folder_items, source_file)?;
    Ok(InventoryNodeRaw::Folder { name: folder_name, items })
}

fn parse_inventory_host(mapping: &Mapping, source_file: &Path) -> io::Result<InventoryHostRaw> {
    let mut host = InventoryHostRaw::default();

    for (raw_key, value) in mapping {
        let original_key = scalar_to_string(raw_key, source_file, "host key")?;
        let canonical_key = canonical_host_key(&original_key);
        match canonical_key {
            "name" => host.name = scalar_to_string(value, source_file, "name")?,
            "description" => host.description = optional_scalar_to_string(value, source_file, "description")?,
            "protocol" => {
                let value = scalar_to_string(value, source_file, "protocol")?;
                host.protocol = crate::inventory::ConnectionProtocol::from_str(&value);
            }
            "host" => host.host = optional_scalar_to_string(value, source_file, "host")?,
            "user" => host.user = optional_scalar_to_string(value, source_file, "user")?,
            "port" => host.port = parse_u16(value, source_file, "port")?,
            "profile" => host.profile = optional_scalar_to_string(value, source_file, "profile")?,
            "vault_pass" => host.vault_pass = optional_scalar_to_string(value, source_file, "vault_pass")?,
            "hidden" => host.hidden = parse_bool(value, source_file, "hidden")?.unwrap_or(false),
            "identity_file" => host.identity_file = optional_scalar_to_string(value, source_file, "identity_file")?,
            "identities_only" => host.identities_only = parse_bool(value, source_file, "identities_only")?,
            "proxy_jump" => host.proxy_jump = optional_scalar_to_string(value, source_file, "proxy_jump")?,
            "proxy_command" => host.proxy_command = optional_scalar_to_string(value, source_file, "proxy_command")?,
            "forward_agent" => host.forward_agent = parse_bool(value, source_file, "forward_agent")?,
            "local_forward" => host.local_forward = parse_string_list(value, source_file, "local_forward", false)?,
            "remote_forward" => host.remote_forward = parse_string_list(value, source_file, "remote_forward", false)?,
            "ssh_options" => merge_ssh_options(&mut host.ssh_options, value, source_file)?,
            "rdp_domain" => host.rdp_domain = optional_scalar_to_string(value, source_file, "rdp_domain")?,
            "rdp_args" => host.rdp_args = parse_string_list(value, source_file, "rdp_args", true)?,
            _ => {
                let value = scalar_to_string(value, source_file, &original_key)?;
                host.ssh_options.insert(original_key, value);
            }
        }
    }

    if host.name.trim().is_empty() {
        return Err(invalid_inventory(source_file, "inventory host is missing required field 'name'"));
    }

    Ok(host)
}

fn merge_ssh_options(into: &mut std::collections::BTreeMap<String, String>, value: &Value, source_file: &Path) -> io::Result<()> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid_inventory(source_file, "ssh_options must be a mapping"));
    };

    for (key, value) in mapping {
        let key = scalar_to_string(key, source_file, "ssh_options key")?;
        let value = scalar_to_string(value, source_file, "ssh_options value")?;
        into.insert(key, value);
    }

    Ok(())
}

fn mapping_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.iter().find_map(|(candidate_key, value)| {
        let Value::String(candidate_key) = candidate_key else {
            return None;
        };
        (canonical_top_level_key(candidate_key) == key).then_some(value)
    })
}

fn mapping_has_key(mapping: &Mapping, key: &str) -> bool {
    mapping.iter().any(|(candidate_key, _)| {
        let Value::String(candidate_key) = candidate_key else {
            return false;
        };
        canonical_host_key(candidate_key) == key
    })
}

fn parse_string_list(value: &Value, source_file: &Path, field: &str, split_scalar: bool) -> io::Result<Vec<String>> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Sequence(sequence) => sequence
            .iter()
            .map(|item| scalar_to_string(item, source_file, field))
            .collect::<io::Result<Vec<_>>>(),
        Value::String(text) if split_scalar => Ok(text.split_whitespace().map(str::to_string).collect()),
        _ => Ok(vec![scalar_to_string(value, source_file, field)?]),
    }
}

fn parse_u16(value: &Value, source_file: &Path, field: &str) -> io::Result<Option<u16>> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => {
            let Some(raw) = number.as_u64() else {
                return Err(invalid_inventory(source_file, format!("{field} must be an unsigned integer")));
            };
            let port = u16::try_from(raw).map_err(|_| invalid_inventory(source_file, format!("{field} value '{raw}' is out of range")))?;
            Ok(Some(port))
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let port = trimmed
                .parse::<u16>()
                .map_err(|_| invalid_inventory(source_file, format!("{field} value '{trimmed}' is not a valid port number")))?;
            Ok(Some(port))
        }
        _ => Err(invalid_inventory(source_file, format!("{field} must be a string or integer"))),
    }
}

fn parse_bool(value: &Value, source_file: &Path, field: &str) -> io::Result<Option<bool>> {
    match value {
        Value::Null => Ok(None),
        Value::Bool(boolean) => Ok(Some(*boolean)),
        Value::Number(number) => match number.as_i64() {
            Some(0) => Ok(Some(false)),
            Some(1) => Ok(Some(true)),
            Some(other) => Err(invalid_inventory(source_file, format!("{field} numeric value '{other}' must be 0 or 1"))),
            None => Err(invalid_inventory(source_file, format!("{field} numeric value is invalid"))),
        },
        Value::String(text) => {
            let normalized = text.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "" => Ok(None),
                "1" | "true" | "yes" | "on" => Ok(Some(true)),
                "0" | "false" | "no" | "off" => Ok(Some(false)),
                _ => Err(invalid_inventory(
                    source_file,
                    format!("{field} value '{}' is not a valid boolean", text.trim()),
                )),
            }
        }
        _ => Err(invalid_inventory(source_file, format!("{field} must be a boolean-like scalar"))),
    }
}

fn optional_scalar_to_string(value: &Value, source_file: &Path, field: &str) -> io::Result<Option<String>> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }

    let value = scalar_to_string(value, source_file, field)?;
    if value.trim().is_empty() { Ok(None) } else { Ok(Some(value)) }
}

fn scalar_to_string(value: &Value, source_file: &Path, field: &str) -> io::Result<String> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::Null => Err(invalid_inventory(source_file, format!("{field} cannot be null"))),
        _ => Err(invalid_inventory(source_file, format!("{field} must be a scalar string, boolean, or number"))),
    }
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

fn invalid_inventory(source_file: &Path, message: impl Into<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("inventory error in '{}': {}", source_file.display(), message.into()),
    )
}

#[cfg(test)]
#[path = "../test/inventory/loader.rs"]
mod tests;
