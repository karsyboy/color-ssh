//! Inventory include expansion and tree construction.

use super::error::{InventoryResult, invalid_inventory};
use super::include::{expand_include_pattern, resolve_include_pattern};
use super::model::{FolderId, InventoryHost, InventoryNodeRaw, InventoryTreeModel, ParsedInventoryDocument, TreeFolder};
use super::normalize::normalize_inventory_host;
use super::parse::parse_inventory_document;
use crate::log_debug;
use std::collections::{HashMap, HashSet};
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

pub(crate) fn build_inventory_tree(inventory_path: &Path) -> io::Result<InventoryTreeModel> {
    build_inventory_tree_internal(inventory_path).map_err(io::Error::from)
}

fn build_inventory_tree_internal(inventory_path: &Path) -> InventoryResult<InventoryTreeModel> {
    log_debug!("Building inventory tree from '{}'", inventory_path.display());

    let root_name = inventory_path
        .file_name()
        .and_then(|segment| segment.to_str())
        .unwrap_or("cossh-inventory.yaml")
        .to_string();
    let mut root = FolderAccumulator::new(root_name, inventory_path.to_path_buf());
    let mut hosts = Vec::new();
    let mut seen_host_names = HashMap::new();
    let mut visited = HashSet::new();

    load_document_recursive(inventory_path, &mut root, &mut hosts, &mut seen_host_names, &mut visited, &[])?;

    log_debug!(
        "Inventory tree build complete: {} host(s) loaded from '{}'",
        hosts.len(),
        inventory_path.display()
    );

    let mut next_id: FolderId = 0;
    let mut tree_root = finalize_folder(root, &mut next_id);
    sort_tree_folder_by_host_name(&mut tree_root, &hosts, |host| host.name.as_str());
    Ok(InventoryTreeModel { root: tree_root, hosts })
}

fn load_document_recursive(
    inventory_path: &Path,
    folder: &mut FolderAccumulator,
    hosts: &mut Vec<InventoryHost>,
    seen_host_names: &mut HashMap<String, PathBuf>,
    visited: &mut HashSet<PathBuf>,
    folder_path: &[String],
) -> InventoryResult<()> {
    let canonical = inventory_path.canonicalize().unwrap_or_else(|_| inventory_path.to_path_buf());

    log_debug!("Loading inventory document '{}'", canonical.display());

    if !visited.insert(canonical.clone()) {
        // Include cycles are ignored once a file has already been loaded.
        log_debug!("Skipping already visited inventory file (possible include cycle): {}", canonical.display());
        return Ok(());
    }

    let ParsedInventoryDocument { include, inventory } = parse_inventory_document(&canonical)?;
    log_debug!(
        "Parsed inventory document '{}' (include count: {}, inventory node count: {})",
        canonical.display(),
        include.len(),
        inventory.len()
    );
    let parent_dir = canonical.parent().unwrap_or(Path::new("."));

    for include_pattern in include {
        let resolved_pattern = resolve_include_pattern(&include_pattern, parent_dir);
        let include_paths = expand_include_pattern(&resolved_pattern);
        if include_paths.is_empty() {
            log_debug!(
                "Inventory include '{}' in '{}' resolved to '{}' but matched no files",
                include_pattern,
                canonical.display(),
                resolved_pattern
            );
        } else {
            log_debug!(
                "Inventory include '{}' in '{}' resolved to '{}' and matched {} file(s)",
                include_pattern,
                canonical.display(),
                resolved_pattern,
                include_paths.len()
            );
        }

        for include_path in include_paths {
            log_debug!("Loading include '{}' referenced by '{}'", include_path.display(), canonical.display());
            load_include_document(&include_path, folder, hosts, seen_host_names, visited, folder_path)?;
        }
    }

    for node in inventory {
        add_inventory_node(node, folder, hosts, seen_host_names, &canonical, folder_path)?;
    }

    Ok(())
}

fn load_include_document(
    inventory_path: &Path,
    parent_folder: &mut FolderAccumulator,
    hosts: &mut Vec<InventoryHost>,
    seen_host_names: &mut HashMap<String, PathBuf>,
    visited: &mut HashSet<PathBuf>,
    parent_folder_path: &[String],
) -> InventoryResult<()> {
    let canonical = inventory_path.canonicalize().unwrap_or_else(|_| inventory_path.to_path_buf());

    log_debug!("Preparing included inventory file '{}'", canonical.display());

    if visited.contains(&canonical) {
        // Child include may point back to an ancestor; skip already-loaded file.
        log_debug!("Skipping already visited inventory file (possible include cycle): {}", canonical.display());
        return Ok(());
    }

    let folder_name = inventory_folder_name(&canonical);
    log_debug!("Attaching include '{}' under folder '{}'", canonical.display(), folder_name);
    let child = parent_folder.child_mut(&folder_name, &canonical);
    let mut child_path = parent_folder_path.to_vec();
    child_path.push(folder_name);
    load_document_recursive(&canonical, child, hosts, seen_host_names, visited, &child_path)
}

fn inventory_folder_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|segment| segment.to_str())
        .unwrap_or("include")
        .to_string()
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

pub(crate) fn sort_tree_folder_by_host_name<T>(folder: &mut TreeFolder, hosts: &[T], host_name: fn(&T) -> &str) {
    folder.host_indices.sort_by(|left_idx, right_idx| {
        let left_name = hosts.get(*left_idx).map(host_name).unwrap_or_default();
        let right_name = hosts.get(*right_idx).map(host_name).unwrap_or_default();
        let left_key = left_name.to_ascii_lowercase();
        let right_key = right_name.to_ascii_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left_name.cmp(right_name))
            .then_with(|| left_idx.cmp(right_idx))
    });

    for child in &mut folder.children {
        sort_tree_folder_by_host_name(child, hosts, host_name);
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
) -> InventoryResult<()> {
    match node {
        InventoryNodeRaw::Host(raw) => {
            let host = normalize_inventory_host(*raw, source_file, folder_path)?;
            // Host aliases are globally unique across merged include files.
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

            log_debug!(
                "Loaded inventory host '{}' (protocol='{}', source='{}')",
                host.name,
                host.protocol.as_str(),
                host.source_file.display()
            );
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

#[cfg(test)]
#[path = "../test/inventory/loader.rs"]
mod tests;
