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
    ModuleError {
        module_index: usize,
        error: String,
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
fn calculate_size(path: &Path) -> u64 {
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

/// Spawn a background task that scans all modules and sends results via the channel.
pub fn start_scan(modules: Vec<Module>, tx: mpsc::UnboundedSender<ScanMessage>) {
    tokio::task::spawn_blocking(move || {
        for (module_index, module) in modules.iter().enumerate() {
            for target in &module.targets {
                let paths = expand_target_path(&target.path);

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

                    if tx.send(ScanMessage::ItemDiscovered { module_index, item }).is_err() {
                        return; // receiver dropped, TUI exited
                    }
                }
            }

            if tx.send(ScanMessage::ModuleComplete { module_index }).is_err() {
                return;
            }
        }

        let _ = tx.send(ScanMessage::ScanComplete);
    });
}
