// Drill-in directory exploration state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::safety;

use super::Item;

/// A level in the drill-in directory exploration stack.
pub struct DrillLevel {
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub parent_selected_index: usize,
}

/// Encapsulates drill-in state (stack + selection cache) and enforces invariants.
#[derive(Default)]
pub struct DrillState {
    stack: Vec<DrillLevel>,
    selection_cache: HashMap<PathBuf, (Option<u64>, safety::SafetyLevel)>,
}

impl DrillState {
    /// Create empty drill state.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            selection_cache: HashMap::new(),
        }
    }

    /// Whether the user is currently drilled into a directory.
    pub fn is_active(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Number of drill levels deep.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Return current drill items if active, otherwise the fallback slice.
    pub fn items_or<'a>(&'a self, fallback: &'a [Item]) -> &'a [Item] {
        if let Some(level) = self.stack.last() {
            &level.items
        } else {
            fallback
        }
    }

    /// Return items at the current drill level, or None if not drilled in.
    pub fn current_items(&self) -> Option<&[Item]> {
        self.stack.last().map(|level| level.items.as_slice())
    }

    /// Push a new drill level (enter a directory).
    pub fn push(&mut self, path: PathBuf, items: Vec<Item>, parent_selected_index: usize) {
        self.stack.push(DrillLevel {
            path,
            items,
            parent_selected_index,
        });
    }

    /// Pop the current drill level, returning the parent's selected index.
    pub fn pop(&mut self) -> Option<usize> {
        self.stack.pop().map(|level| level.parent_selected_index)
    }

    /// Update a drill item's size (from a `DrillItemSized` message) and sync the selection cache.
    pub fn update_item_size(&mut self, depth: usize, item_index: usize, size: u64) {
        if let Some(level) = self.stack.get_mut(depth) {
            if let Some(item) = level.items.get_mut(item_index) {
                item.size = Some(size);
                if let Some(cached) = self.selection_cache.get_mut(&item.path) {
                    cached.0 = Some(size);
                }
            }
        }
    }

    /// Cache metadata for a selected drill item so size/safety survives stack pops.
    pub fn cache_selection(&mut self, path: PathBuf, meta: (Option<u64>, safety::SafetyLevel)) {
        self.selection_cache.insert(path, meta);
    }

    /// Remove cached metadata for a deselected drill item.
    pub fn uncache_selection(&mut self, path: &Path) {
        self.selection_cache.remove(path);
    }

    /// Look up size and safety for a path: checks stack first, then cache.
    pub fn lookup_meta(&self, path: &Path) -> Option<(Option<u64>, safety::SafetyLevel)> {
        for level in &self.stack {
            if let Some(item) = level.items.iter().find(|i| i.path == path) {
                return Some((item.size, item.safety_level));
            }
        }
        self.selection_cache.get(path).copied()
    }

    /// Collect all drill item sizes into a map (for parent size adjustment during cleanup).
    pub fn collect_item_sizes(&self) -> HashMap<PathBuf, u64> {
        self.stack
            .iter()
            .flat_map(|level| level.items.iter())
            .filter_map(|item| item.size.map(|s| (item.path.clone(), s)))
            .collect()
    }

    /// Return breadcrumb directory names from the drill stack.
    pub fn breadcrumb_parts(&self) -> Vec<String> {
        self.stack
            .iter()
            .map(|level| {
                level
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| level.path.display().to_string())
            })
            .collect()
    }

    /// Return paths at the given drill depth for background size scanning.
    pub fn scan_paths_at_depth(&self, depth: usize) -> Vec<PathBuf> {
        self.stack
            .get(depth)
            .map(|level| level.items.iter().map(|i| i.path.clone()).collect())
            .unwrap_or_default()
    }

    /// Reset all drill state.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.selection_cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ItemType;

    fn make_item(name: &str, path: &str, size: Option<u64>) -> Item {
        Item {
            name: name.into(),
            path: PathBuf::from(path),
            size,
            item_type: ItemType::File,
            target_description: None,
            safety_level: crate::core::safety::SafetyLevel::Safe,
            is_shared: false,
            restore_kind: crate::module::manifest::RestoreKind::default(),
            restore_steps: None,
            risk_level: crate::module::manifest::RiskLevel::default(),
        }
    }

    #[test]
    fn drill_state_push_pop_restores_index() {
        let mut ds = DrillState::new();
        assert!(!ds.is_active());
        ds.push(PathBuf::from("/a"), vec![], 5);
        assert!(ds.is_active());
        assert_eq!(ds.depth(), 1);
        let idx = ds.pop();
        assert_eq!(idx, Some(5));
        assert!(!ds.is_active());
    }

    #[test]
    fn drill_state_items_or_fallback() {
        let ds = DrillState::new();
        let fallback = vec![make_item("x", "/x", Some(1))];
        assert_eq!(ds.items_or(&fallback).len(), 1);
    }

    #[test]
    fn drill_state_items_or_drill() {
        let mut ds = DrillState::new();
        let drill_items = vec![make_item("a", "/a", None), make_item("b", "/b", None)];
        ds.push(PathBuf::from("/dir"), drill_items, 0);
        let fallback: Vec<Item> = vec![];
        assert_eq!(ds.items_or(&fallback).len(), 2);
    }

    #[test]
    fn drill_state_update_item_size_syncs_cache() {
        let mut ds = DrillState::new();
        let items = vec![make_item("f", "/f", None)];
        ds.push(PathBuf::from("/dir"), items, 0);
        ds.cache_selection(
            PathBuf::from("/f"),
            (None, crate::core::safety::SafetyLevel::Safe),
        );
        ds.update_item_size(0, 0, 42);
        let meta = ds.lookup_meta(Path::new("/f")).unwrap();
        assert_eq!(meta.0, Some(42));
    }

    #[test]
    fn drill_state_cache_uncache() {
        let mut ds = DrillState::new();
        ds.cache_selection(
            PathBuf::from("/x"),
            (Some(10), crate::core::safety::SafetyLevel::Warn),
        );
        assert!(ds.lookup_meta(Path::new("/x")).is_some());
        ds.uncache_selection(Path::new("/x"));
        assert!(ds.lookup_meta(Path::new("/x")).is_none());
    }

    #[test]
    fn drill_state_clear_resets() {
        let mut ds = DrillState::new();
        ds.push(PathBuf::from("/a"), vec![], 0);
        ds.cache_selection(
            PathBuf::from("/x"),
            (Some(1), crate::core::safety::SafetyLevel::Safe),
        );
        ds.clear();
        assert!(!ds.is_active());
        assert!(ds.lookup_meta(Path::new("/x")).is_none());
    }

    #[test]
    fn drill_state_lookup_meta_stack_first() {
        let mut ds = DrillState::new();
        let items = vec![make_item("f", "/f", Some(100))];
        ds.push(PathBuf::from("/dir"), items, 0);
        // Cache has a different value — stack should win
        ds.cache_selection(
            PathBuf::from("/f"),
            (Some(999), crate::core::safety::SafetyLevel::Warn),
        );
        let meta = ds.lookup_meta(Path::new("/f")).unwrap();
        assert_eq!(meta.0, Some(100));
        assert_eq!(meta.1, crate::core::safety::SafetyLevel::Safe);
    }

    #[test]
    fn drill_state_breadcrumb_parts() {
        let mut ds = DrillState::new();
        ds.push(PathBuf::from("/a/dir1"), vec![], 0);
        ds.push(PathBuf::from("/a/dir1/sub"), vec![], 0);
        let parts = ds.breadcrumb_parts();
        assert_eq!(parts, vec!["dir1", "sub"]);
    }

    #[test]
    fn drill_state_collect_item_sizes() {
        let mut ds = DrillState::new();
        let items = vec![make_item("a", "/a", Some(10)), make_item("b", "/b", None)];
        ds.push(PathBuf::from("/dir"), items, 0);
        let sizes = ds.collect_item_sizes();
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes[&PathBuf::from("/a")], 10);
    }
}
