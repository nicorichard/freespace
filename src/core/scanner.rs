// Filesystem scanner for discovering items and calculating sizes.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::app::{Item, ItemType};
use crate::module::manifest::Module;

/// Messages sent from the scanner to the TUI event loop.
pub enum ScanMessage {
    /// A new item was discovered with its calculated size.
    ItemDiscovered { module_index: usize, item: Item },
    /// All items for a module have been discovered and sized.
    ModuleComplete { module_index: usize },
    /// An error occurred while scanning a module.
    #[allow(dead_code)]
    ModuleError { module_index: usize, error: String },
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
pub(crate) fn expand_tilde(pattern: &str) -> String {
    if pattern.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &pattern[1..]);
        }
    }
    pattern.to_string()
}

/// Expand a target path pattern into concrete filesystem paths.
/// Handles `~` for home directory and `*` for glob expansion.
pub(crate) fn expand_target_path(pattern: &str) -> Vec<PathBuf> {
    let expanded = expand_tilde(pattern);

    match glob::glob(&expanded) {
        Ok(paths) => paths.filter_map(|p| p.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Return the actual disk usage of a file from its metadata.
/// On Unix, this uses block count to handle sparse files correctly (like `du`).
/// On other platforms, falls back to the logical file length.
#[cfg(unix)]
fn file_disk_size(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn file_disk_size(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

/// Calculate the size of a file or directory.
/// Uses actual disk usage (not apparent size) to correctly handle sparse files.
pub fn calculate_size(path: &Path) -> u64 {
    if path.is_file() {
        std::fs::metadata(path)
            .map(|m| file_disk_size(&m))
            .unwrap_or(0)
    } else if path.is_dir() {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.metadata().map(|m| file_disk_size(&m)).unwrap_or(0))
            .sum()
    } else {
        0
    }
}

/// Discover local directories matching `dir_name` under the given search roots.
/// If `indicator` is set, only matches where the parent contains the indicator file.
pub(crate) fn discover_local_dirs(
    dir_name: &str,
    search_roots: &[PathBuf],
    indicator: Option<&str>,
) -> Vec<PathBuf> {
    let mut results = Vec::new();
    for root in search_roots {
        let mut it = walkdir::WalkDir::new(root).follow_links(false).into_iter();
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
pub(crate) fn local_item_name(path: &Path, dir_name: &str) -> String {
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
                    let paths =
                        discover_local_dirs(dir_name, &search_dirs, target.indicator.as_deref());

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- calculate_size ---

    #[test]
    fn calculate_size_empty_dir() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(calculate_size(tmp.path()), 0);
    }

    #[test]
    fn calculate_size_single_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("data.bin");
        fs::write(&file, vec![0u8; 1024]).unwrap();
        // Disk usage may be >= written bytes due to block alignment
        assert!(calculate_size(&file) >= 1024);
    }

    #[test]
    fn calculate_size_nested_directory() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("a.txt"), vec![0u8; 100]).unwrap();
        fs::write(sub.join("b.txt"), vec![0u8; 200]).unwrap();
        fs::write(tmp.path().join("root.txt"), vec![0u8; 50]).unwrap();
        // Disk usage may be >= written bytes due to block alignment
        assert!(calculate_size(tmp.path()) >= 350);
    }

    #[test]
    fn calculate_size_nonexistent_path() {
        assert_eq!(calculate_size(Path::new("/nonexistent/path/xyz")), 0);
    }

    // --- expand_tilde ---

    #[test]
    fn expand_tilde_with_home() {
        let result = expand_tilde("~/Documents");
        // Should expand on any system with HOME set
        if std::env::var("HOME").is_ok() {
            assert!(!result.starts_with("~"));
            assert!(result.ends_with("/Documents"));
        }
    }

    #[test]
    fn expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/usr/local"), "/usr/local");
    }

    // --- expand_target_path ---

    #[test]
    fn expand_target_path_no_glob() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("specific");
        fs::create_dir(&dir).unwrap();
        let paths = expand_target_path(dir.to_str().unwrap());
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], dir);
    }

    #[test]
    fn expand_target_path_with_glob() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("aaa")).unwrap();
        fs::create_dir(tmp.path().join("bbb")).unwrap();
        fs::write(tmp.path().join("file.txt"), b"").unwrap();
        let pattern = format!("{}/*", tmp.path().display());
        let paths = expand_target_path(&pattern);
        assert!(paths.len() >= 2);
    }

    // --- discover_local_dirs ---

    #[test]
    fn discover_local_dirs_matching() {
        let tmp = TempDir::new().unwrap();
        // Create: project/node_modules/
        let project = tmp.path().join("project");
        fs::create_dir_all(project.join("node_modules")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();

        let results = discover_local_dirs(
            "node_modules",
            &[tmp.path().to_path_buf()],
            Some("package.json"),
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].ends_with("node_modules"));
    }

    #[test]
    fn discover_local_dirs_skips_hidden() {
        let tmp = TempDir::new().unwrap();
        // Create: .hidden/node_modules/ — should be skipped
        let hidden = tmp.path().join(".hidden");
        fs::create_dir_all(hidden.join("node_modules")).unwrap();

        let results = discover_local_dirs("node_modules", &[tmp.path().to_path_buf()], None);
        assert!(results.is_empty());
    }

    #[test]
    fn discover_local_dirs_without_indicator() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("project");
        fs::create_dir_all(project.join("target")).unwrap();

        let results = discover_local_dirs("target", &[tmp.path().to_path_buf()], None);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn discover_local_dirs_missing_indicator() {
        let tmp = TempDir::new().unwrap();
        // Create target dir but no Cargo.toml indicator
        let project = tmp.path().join("project");
        fs::create_dir_all(project.join("target")).unwrap();

        let results =
            discover_local_dirs("target", &[tmp.path().to_path_buf()], Some("Cargo.toml"));
        assert!(results.is_empty());
    }

    // --- local_item_name ---

    #[test]
    fn local_item_name_with_parent() {
        let path = Path::new("/home/user/myproject/node_modules");
        assert_eq!(
            local_item_name(path, "node_modules"),
            "myproject/node_modules"
        );
    }

    #[test]
    fn local_item_name_no_parent() {
        let path = Path::new("node_modules");
        // Parent is "" which has no file_name
        let name = local_item_name(path, "node_modules");
        assert_eq!(name, "node_modules");
    }

    // --- start_scan (async) ---

    #[tokio::test]
    async fn start_scan_sends_messages() {
        let tmp = TempDir::new().unwrap();
        let target_dir = tmp.path().join("cache");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("file.dat"), vec![0u8; 512]).unwrap();

        let module = Module {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "test module".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            targets: vec![crate::module::manifest::Target {
                path: Some(target_dir.to_str().unwrap().to_string()),
                name: None,
                indicator: None,
                description: None,
            }],
        };

        let (tx, mut rx) = mpsc::unbounded_channel();
        start_scan(vec![module], tx, vec![]);

        let mut got_item = false;
        let mut got_complete = false;
        let mut got_scan_complete = false;

        // Collect messages with a timeout
        let timeout = tokio::time::sleep(std::time::Duration::from_secs(5));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(ScanMessage::ItemDiscovered { module_index, item }) => {
                            assert_eq!(module_index, 0);
                            assert_eq!(item.name, "cache");
                            // Disk usage may be >= written bytes due to block alignment
                            assert!(item.size.unwrap() >= 512);
                            got_item = true;
                        }
                        Some(ScanMessage::ModuleComplete { module_index }) => {
                            assert_eq!(module_index, 0);
                            got_complete = true;
                        }
                        Some(ScanMessage::ScanComplete) => {
                            got_scan_complete = true;
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
                _ = &mut timeout => {
                    panic!("scan timed out");
                }
            }
        }

        assert!(got_item, "expected ItemDiscovered");
        assert!(got_complete, "expected ModuleComplete");
        assert!(got_scan_complete, "expected ScanComplete");
    }
}
