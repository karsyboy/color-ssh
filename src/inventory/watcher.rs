//! Inventory file watch planning helpers.

use super::error::InventoryResult;
use super::include::{expand_include_pattern, resolve_include_pattern};
use super::parse::parse_inventory_document;
use notify::Event;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryWatchPlan {
    pub(crate) tracked_files: Vec<PathBuf>,
    pub(crate) include_dirs: Vec<PathBuf>,
    pub(crate) watch_paths: Vec<PathBuf>,
}

pub(crate) fn build_inventory_watch_plan(inventory_path: &Path) -> io::Result<InventoryWatchPlan> {
    build_inventory_watch_plan_internal(inventory_path).map_err(io::Error::from)
}

fn build_inventory_watch_plan_internal(inventory_path: &Path) -> InventoryResult<InventoryWatchPlan> {
    let root_path = canonicalize_or_self(inventory_path);
    let mut tracked_files = vec![root_path.clone()];
    let mut include_dirs = Vec::new();
    let mut visited = HashSet::new();

    collect_watch_targets(&root_path, &mut tracked_files, &mut include_dirs, &mut visited)?;
    sort_and_dedup_paths(&mut tracked_files);
    sort_and_dedup_paths(&mut include_dirs);

    let mut watch_paths = include_dirs.iter().map(|path| existing_watch_path(path)).collect::<Vec<_>>();
    watch_paths.extend(tracked_files.iter().map(|path| existing_watch_path(path.parent().unwrap_or(Path::new(".")))));
    sort_and_dedup_paths(&mut watch_paths);

    Ok(InventoryWatchPlan {
        tracked_files,
        include_dirs,
        watch_paths,
    })
}

fn collect_watch_targets(
    inventory_path: &Path,
    tracked_files: &mut Vec<PathBuf>,
    include_dirs: &mut Vec<PathBuf>,
    visited: &mut HashSet<PathBuf>,
) -> InventoryResult<()> {
    let canonical = canonicalize_or_self(inventory_path);
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }

    if !tracked_files.iter().any(|path| path == &canonical) {
        tracked_files.push(canonical.clone());
    }

    if !canonical.is_file() {
        return Ok(());
    }

    let document = parse_inventory_document(&canonical)?;
    let parent_dir = canonical.parent().unwrap_or(Path::new("."));

    for include_pattern in document.include {
        let resolved_pattern = resolve_include_pattern(&include_pattern, parent_dir);
        let resolved_path = PathBuf::from(&resolved_pattern);
        let include_dir = resolved_path
            .parent()
            .map(canonicalize_or_self)
            .unwrap_or_else(|| canonicalize_or_self(parent_dir));
        include_dirs.push(include_dir);

        let include_paths = expand_include_pattern(&resolved_pattern);
        if include_paths.is_empty() && !pattern_contains_wildcard(&resolved_pattern) {
            tracked_files.push(canonicalize_or_self(&resolved_path));
        }

        for include_path in include_paths {
            collect_watch_targets(&include_path, tracked_files, include_dirs, visited)?;
        }
    }

    Ok(())
}

pub(crate) fn should_reload_for_inventory_event(event: &Event, plan: &InventoryWatchPlan) -> bool {
    if !(event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove()) {
        return false;
    }

    if event
        .paths
        .iter()
        .any(|path| plan.tracked_files.iter().any(|tracked| paths_match(path, tracked)))
    {
        return true;
    }

    if event.paths.iter().any(|path| plan.include_dirs.iter().any(|dir| paths_match(path, dir))) {
        return true;
    }

    event
        .paths
        .iter()
        .any(|path| is_yaml_file(path) && path.parent().is_some_and(|parent| plan.include_dirs.iter().any(|dir| paths_match(parent, dir))))
}

fn pattern_contains_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

fn is_yaml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|segment| segment.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yaml" | "yml"))
        .unwrap_or(false)
}

fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn existing_watch_path(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return canonicalize_or_self(&current);
        }
        if !current.pop() {
            return std::env::current_dir()
                .map(|dir| canonicalize_or_self(&dir))
                .unwrap_or_else(|_| PathBuf::from("."));
        }
    }
}

fn paths_match(left: &Path, right: &Path) -> bool {
    left == right || canonicalize_or_self(left) == canonicalize_or_self(right)
}

fn sort_and_dedup_paths(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
}

#[cfg(test)]
#[path = "../test/inventory/watcher.rs"]
mod tests;
