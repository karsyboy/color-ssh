use super::error::{InventoryResult, invalid_inventory};
use super::model::{ConnectionProtocol, InventoryHostRaw, InventoryNodeRaw, ParsedInventoryDocument};
use serde_yml::{Mapping, Value};
use std::fs;
use std::path::Path;

pub(super) fn parse_inventory_document(inventory_path: &Path) -> InventoryResult<ParsedInventoryDocument> {
    let contents = fs::read_to_string(inventory_path)
        .map_err(|err| invalid_inventory(inventory_path, format!("failed to read inventory YAML '{}': {err}", inventory_path.display())))?;
    let yaml: Value = serde_yml::from_str(&contents)
        .map_err(|err| invalid_inventory(inventory_path, format!("failed to parse inventory YAML '{}': {err}", inventory_path.display())))?;
    parse_inventory_document_value(&yaml, inventory_path)
}

fn parse_inventory_document_value(yaml: &Value, source_file: &Path) -> InventoryResult<ParsedInventoryDocument> {
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

    Ok(ParsedInventoryDocument { include, inventory })
}

fn parse_inventory_nodes(value: &Value, source_file: &Path) -> InventoryResult<Vec<InventoryNodeRaw>> {
    let Value::Sequence(sequence) = value else {
        return Err(invalid_inventory(source_file, "inventory must be a YAML list"));
    };

    sequence.iter().map(|item| parse_inventory_node(item, source_file)).collect()
}

fn parse_inventory_node(value: &Value, source_file: &Path) -> InventoryResult<InventoryNodeRaw> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid_inventory(
            source_file,
            "inventory entries must be mappings containing a host definition or folder item",
        ));
    };

    if mapping_has_key(mapping, "name") {
        return Ok(InventoryNodeRaw::Host(Box::new(parse_inventory_host(mapping, source_file)?)));
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

fn parse_inventory_host(mapping: &Mapping, source_file: &Path) -> InventoryResult<InventoryHostRaw> {
    let mut host = InventoryHostRaw::default();

    for (raw_key, value) in mapping {
        let original_key = scalar_to_string(raw_key, source_file, "host key")?;
        let canonical_key = canonical_host_key(&original_key);
        match canonical_key {
            "name" => host.name = scalar_to_string(value, source_file, "name")?,
            "description" => host.description = optional_scalar_to_string(value, source_file, "description")?,
            "protocol" => {
                let value = scalar_to_string(value, source_file, "protocol")?;
                host.protocol = ConnectionProtocol::from(value.as_str());
            }
            "host" => host.host = optional_scalar_to_string(value, source_file, "host")?,
            "user" => host.user = optional_scalar_to_string(value, source_file, "user")?,
            "port" => host.port = parse_u16(value, source_file, "port")?,
            "profile" => host.profile = optional_scalar_to_string(value, source_file, "profile")?,
            "vault_pass" => host.vault_pass = optional_scalar_to_string(value, source_file, "vault_pass")?,
            "hidden" => host.hidden = parse_bool(value, source_file, "hidden")?.unwrap_or(false),
            "identity_file" => host.identity_files = parse_string_list(value, source_file, "identity_file", false)?,
            "identities_only" => host.identities_only = parse_bool(value, source_file, "identities_only")?,
            "proxy_jump" => host.proxy_jump = optional_scalar_to_string(value, source_file, "proxy_jump")?,
            "proxy_command" => host.proxy_command = optional_scalar_to_string(value, source_file, "proxy_command")?,
            "forward_agent" => host.forward_agent = parse_forward_agent(value, source_file)?,
            "local_forward" => host.local_forward = parse_string_list(value, source_file, "local_forward", false)?,
            "remote_forward" => host.remote_forward = parse_string_list(value, source_file, "remote_forward", false)?,
            "ssh_options" => merge_ssh_options(&mut host, value, source_file)?,
            "rdp_domain" => host.rdp_domain = optional_scalar_to_string(value, source_file, "rdp_domain")?,
            "rdp_args" => host.rdp_args = parse_string_list(value, source_file, "rdp_args", true)?,
            _ => {
                let values = parse_ssh_option_values(value, source_file, &original_key)?;
                if !values.is_empty() {
                    host.ssh_options.insert(original_key, values);
                }
            }
        }
    }

    if host.name.trim().is_empty() {
        return Err(invalid_inventory(source_file, "inventory host is missing required field 'name'"));
    }

    Ok(host)
}

fn merge_ssh_options(into: &mut InventoryHostRaw, value: &Value, source_file: &Path) -> InventoryResult<()> {
    let Value::Mapping(mapping) = value else {
        return Err(invalid_inventory(source_file, "ssh_options must be a mapping"));
    };

    for (key, value) in mapping {
        let key = scalar_to_string(key, source_file, "ssh_options key")?;
        match compact_key(&key).as_str() {
            "identityfile" => into
                .identity_files
                .extend(parse_string_list(value, source_file, "ssh_options IdentityFile", false)?),
            "identitiesonly" => into.identities_only = parse_bool(value, source_file, "ssh_options IdentitiesOnly")?,
            "proxyjump" => into.proxy_jump = optional_scalar_to_string(value, source_file, "ssh_options ProxyJump")?,
            "proxycommand" => into.proxy_command = optional_scalar_to_string(value, source_file, "ssh_options ProxyCommand")?,
            "forwardagent" => into.forward_agent = parse_forward_agent(value, source_file)?,
            "localforward" => into
                .local_forward
                .extend(parse_string_list(value, source_file, "ssh_options LocalForward", false)?),
            "remoteforward" => into
                .remote_forward
                .extend(parse_string_list(value, source_file, "ssh_options RemoteForward", false)?),
            _ => {
                let values = parse_ssh_option_values(value, source_file, "ssh_options value")?;
                if !values.is_empty() {
                    into.ssh_options.entry(key).or_default().extend(values);
                }
            }
        }
    }

    Ok(())
}

fn parse_forward_agent(value: &Value, source_file: &Path) -> InventoryResult<Option<String>> {
    optional_scalar_to_string(value, source_file, "forward_agent").map(|value| value.map(|text| normalize_yes_no_string(&text)))
}

fn parse_ssh_option_values(value: &Value, source_file: &Path, field: &str) -> InventoryResult<Vec<String>> {
    match value {
        Value::Null => Err(invalid_inventory(source_file, format!("{field} cannot be null"))),
        Value::Sequence(sequence) => sequence.iter().map(|item| ssh_option_scalar_to_string(item, source_file, field)).collect(),
        _ => Ok(vec![ssh_option_scalar_to_string(value, source_file, field)?]),
    }
}

fn ssh_option_scalar_to_string(value: &Value, source_file: &Path, field: &str) -> InventoryResult<String> {
    match value {
        Value::Bool(boolean) => Ok(if *boolean { "yes".to_string() } else { "no".to_string() }),
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) => Ok(text.clone()),
        Value::Null => Err(invalid_inventory(source_file, format!("{field} cannot be null"))),
        _ => Err(invalid_inventory(source_file, format!("{field} must be a scalar string, boolean, or number"))),
    }
}

fn normalize_yes_no_string(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => "yes".to_string(),
        "0" | "false" | "no" | "off" => "no".to_string(),
        _ => value.trim().to_string(),
    }
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

fn parse_string_list(value: &Value, source_file: &Path, field: &str, split_scalar: bool) -> InventoryResult<Vec<String>> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Sequence(sequence) => sequence
            .iter()
            .map(|item| scalar_to_string(item, source_file, field))
            .collect::<InventoryResult<Vec<_>>>(),
        Value::String(text) if split_scalar => Ok(text.split_whitespace().map(str::to_string).collect()),
        _ => Ok(vec![scalar_to_string(value, source_file, field)?]),
    }
}

fn parse_u16(value: &Value, source_file: &Path, field: &str) -> InventoryResult<Option<u16>> {
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

fn parse_bool(value: &Value, source_file: &Path, field: &str) -> InventoryResult<Option<bool>> {
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

fn optional_scalar_to_string(value: &Value, source_file: &Path, field: &str) -> InventoryResult<Option<String>> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }

    let value = scalar_to_string(value, source_file, field)?;
    if value.trim().is_empty() { Ok(None) } else { Ok(Some(value)) }
}

fn scalar_to_string(value: &Value, source_file: &Path, field: &str) -> InventoryResult<String> {
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
