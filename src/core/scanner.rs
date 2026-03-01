// Filesystem scanner for discovering items and calculating sizes.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::app::{Item, ItemType};
use crate::module::manifest::Module;

/// Messages sent from the scanner to the TUI event loop.
pub enum ScanMessage {
    /// A new item was discovered with its calculated size.
    ItemDiscovered {
        module_index: usize,
        item: Item,
    },
    /// All items for a module have been discovered and sized.
    ModuleComplete {
        module_index: usize,
    },
    /// An error occurred while scanning a module.
    #[allow(dead_code)]
    ModuleError {
        module_index: usize,
        error: String,
    },
    /// A drill-in item's size has been calculated.
    DrillItemSized {
        drill_depth: usize,
        item_index: usize,
        size: u64,
    },
    /// All modules have been scanned.
    ScanComplete,
}

/// Expand `~` in a path pattern to the user's home directory.
fn expand_tilde(pattern: &str) -> String {
    if pattern.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &pattern[1..]);
        }
    }
    pattern.to_string()
}

/// Expand a target path pattern into concrete filesystem paths.
/// Handles `~` for home directory and `*` for glob expansion.
fn expand_target_path(pattern: &str) -> Vec<PathBuf> {
    let expanded = expand_tilde(pattern);

    match glob::glob(&expanded) {
        Ok(paths) => paths.filter_map(|p| p.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Calculate the size of a file or directory.
pub fn calculate_size(path: &Path) -> u64 {
    if path.is_file() {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    } else if path.is_dir() {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum()
    } else {
        0
    }
}

/// Discover local directories matching `dir_name` under the given search roots.
/// If `indicator` is set, only matches where the parent contains the indicator file.
fn discover_local_dirs(
    dir_name: &str,
    search_roots: &[PathBuf],
    indicator: Option<&str>,
) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for root in search_roots {
        let mut it = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter();
        while let Some(entry) = it.next() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            // Skip hidden dirs (unless it IS the target we're looking for)
            if entry.depth() > 0 && name.starts_with('.') && name != dir_name {
                it.skip_current_dir();
                continue;
            }
            if name == dir_name {
                if let Some(ind) = indicator {
                    if !entry
                        .path()
                        .parent()
                        .map(|p| p.join(ind).exists())
                        .unwrap_or(false)
                    {
                        it.skip_current_dir();
                        continue;
                    }
                }
                results.push(entry.into_path());
                it.skip_current_dir(); // don't recurse into matched dir
            }
        }
    }
    results
}

/// Build a display name for a locally-discovered directory using the project
/// context: `project-name/dir_name` derived from the parent directory.
fn local_item_name(path: &Path, dir_name: &str) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .map(|p| format!("{}/{}", p.to_string_lossy(), dir_name))
        .unwrap_or_else(|| dir_name.to_string())
}

/// Spawn a background task that scans all modules and sends results via the channel.
pub fn start_scan(
    modules: Vec<Module>,
    tx: mpsc::UnboundedSender<ScanMessage>,
    search_dirs: Vec<PathBuf>,
) {
    tokio::task::spawn_blocking(move || {
        for (module_index, module) in modules.iter().enumerate() {
            for target in &module.targets {
                if let Some(ref path_pattern) = target.path {
                    // Global target: expand path pattern
                    let paths = expand_target_path(path_pattern);

                    for path in paths {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());

                        let item_type = if path.is_dir() {
                            ItemType::Directory
                        } else {
                            ItemType::File
                        };

                        let size = calculate_size(&path);

                        let item = Item {
                            name,
                            path,
                            size: Some(size),
                            item_type,
                        };

                        if tx
                            .send(ScanMessage::ItemDiscovered { module_index, item })
                            .is_err()
                        {
                            return;
                        }
                    }
                } else if let Some(ref dir_name) = target.name {
                    // Local target: discover directories by name
                    let paths = discover_local_dirs(
                        dir_name,
                        &search_dirs,
                        target.indicator.as_deref(),
                    );

                    for path in paths {
                        let name = local_item_name(&path, dir_name);
                        let size = calculate_size(&path);

                        let item = Item {
                            name,
                            path,
                            size: Some(size),
                            item_type: ItemType::Directory,
                        };

                        if tx
                            .send(ScanMessage::ItemDiscovered { module_index, item })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            if tx
                .send(ScanMessage::ModuleComplete { module_index })
                .is_err()
            {
                return;
            }
        }

        let _ = tx.send(ScanMessage::ScanComplete);
    });
}
