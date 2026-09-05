// File and directory cleanup operations.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc;

use crate::core::audit;
use crate::core::safety;

/// Messages sent from background cleanup task to the UI.
pub enum CleanupMessage {
    /// One item has been processed (success or failure).
    Progress {
        done: usize,
        total: usize,
        path: PathBuf,
        /// What to show for this step. `None` means fall back to the path.
        /// Handler steps set this to the command being run, since their path is
        /// a synthetic identity that would mean nothing to the user.
        label: Option<String>,
    },
    /// The entire cleanup operation has finished (or was cancelled).
    Complete(CleanupResult),
}

/// How an item is removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CleanupAction {
    /// Trash or unlink the item's path. Subject to the full path safety check.
    #[default]
    Path,
    /// Delegate to a built-in handler, which removes the item through its own
    /// tool's supported interface.
    Handler {
        handler: &'static str,
        /// Identifier this handler produced during discovery (e.g. a UDID).
        id: String,
    },
}

impl CleanupAction {
    /// Whether removal can be undone. Handler commands have no Trash
    /// equivalent, so they are always permanent.
    pub fn is_reversible(&self) -> bool {
        matches!(self, CleanupAction::Path)
    }

    /// The exact command a handler action will run, for display before the user
    /// confirms. `None` for path actions.
    pub fn removal_command(&self) -> Option<String> {
        match self {
            CleanupAction::Path => None,
            CleanupAction::Handler { handler, id } => {
                crate::core::handlers::get(handler).map(|h| h.describe_removal(id))
            }
        }
    }
}

/// Options controlling cleanup behavior.
pub struct CleanupOptions {
    pub dry_run: bool,
    pub protected_paths: Vec<PathBuf>,
    pub audit_log: bool,
    /// Whether to enforce that paths must be under $HOME.
    pub enforce_scope: bool,
    /// Whether to allow operations on warn-tier paths (user confirmed).
    pub allow_warned: bool,
}

impl Default for CleanupOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            protected_paths: Vec::new(),
            audit_log: true,
            enforce_scope: true,
            allow_warned: false,
        }
    }
}

/// An item to be cleaned up, with optional ignore patterns.
pub struct CleanupItem {
    /// The item's identity. A real filesystem path for [`CleanupAction::Path`];
    /// a synthetic identity for handler items, which is never touched on disk.
    pub path: PathBuf,
    /// Glob patterns for files/directories to preserve within this path.
    pub ignore_patterns: Vec<String>,
    /// The module that owns this item (for audit logging).
    pub module_id: String,
    /// Known size of this item in bytes (for audit logging).
    pub size: Option<u64>,
    /// How to remove it.
    pub action: CleanupAction,
}

impl From<PathBuf> for CleanupItem {
    fn from(path: PathBuf) -> Self {
        Self {
            path,
            ignore_patterns: Vec::new(),
            module_id: String::new(),
            size: None,
            action: CleanupAction::Path,
        }
    }
}

/// Split items so every reversible (path) removal runs before any irreversible
/// (handler) one.
///
/// Cancelling partway through then leaves the permanent operations undone,
/// which is the failure mode worth protecting: a half-finished trash run is
/// recoverable, a half-finished `simctl delete` run is not.
fn reversible_first(items: &[CleanupItem]) -> Vec<&CleanupItem> {
    let (reversible, irreversible): (Vec<_>, Vec<_>) =
        items.iter().partition(|i| i.action.is_reversible());
    reversible.into_iter().chain(irreversible).collect()
}

/// Run a handler removal for one item.
///
/// Deliberately **not** subject to `check_safety`: the path-based safety model
/// answers "is it safe to unlink this?", which is the wrong question for an
/// operation that does not unlink anything. The handler is compiled-in,
/// reviewed code operating on an identifier it produced itself. Authority is
/// the `action` variant — never the shape of the identity path.
fn run_handler(
    handler_id: &str,
    item_id: &str,
    item: &CleanupItem,
    opts: &CleanupOptions,
) -> Result<(), String> {
    let Some(handler) = crate::core::handlers::get(handler_id) else {
        return Err(format!("unknown handler '{handler_id}'"));
    };

    if opts.dry_run {
        return Ok(());
    }

    handler.remove(item_id).map_err(|e| e.to_string())?;

    if opts.audit_log {
        audit::log_operation(
            &handler.describe_removal(item_id),
            &item.path,
            item.size,
            &item.module_id,
        );
    }
    Ok(())
}

/// Result of a cleanup operation.
pub struct CleanupResult {
    pub succeeded: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

/// Move the given files and directories to the system trash, returning which succeeded and which failed.
///
/// Checks `cancel` between each item and stops early if set.
/// Sends `CleanupMessage::Progress` after each item and `CleanupMessage::Complete` when done.
pub fn trash_items(
    items: &[CleanupItem],
    opts: &CleanupOptions,
    cancel: &AtomicBool,
    progress_tx: &mpsc::UnboundedSender<CleanupMessage>,
) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    let total = items.len();

    for (i, item) in reversible_first(items).into_iter().enumerate() {
        let path = &item.path;
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        if let CleanupAction::Handler { handler, id } = &item.action {
            match run_handler(handler, id, item, opts) {
                Ok(()) => result.succeeded.push(path.clone()),
                Err(e) => result.failed.push((path.clone(), e)),
            }
        } else if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
        } else if opts.dry_run {
            result.succeeded.push(path.clone());
        } else if !item.ignore_patterns.is_empty() && path.is_dir() {
            match trash_dir_filtered(path, &item.ignore_patterns) {
                Ok(()) => {
                    if opts.audit_log {
                        audit::log_operation("TRASH", path, item.size, &item.module_id);
                    }
                    result.succeeded.push(path.clone());
                }
                Err(e) => result.failed.push((path.clone(), e.to_string())),
            }
        } else {
            match trash::delete(path) {
                Ok(()) => {
                    if opts.audit_log {
                        audit::log_operation("TRASH", path, item.size, &item.module_id);
                    }
                    result.succeeded.push(path.clone());
                }
                Err(e) => result.failed.push((path.clone(), e.to_string())),
            }
        }

        let _ = progress_tx.send(CleanupMessage::Progress {
            done: i + 1,
            total,
            path: path.clone(),
            label: item.action.removal_command(),
        });
    }

    result
}

/// Delete the given files and directories, returning which succeeded and which failed.
///
/// Checks `cancel` between each item and stops early if set.
/// Sends `CleanupMessage::Progress` after each item and `CleanupMessage::Complete` when done.
pub fn delete_items(
    items: &[CleanupItem],
    opts: &CleanupOptions,
    cancel: &AtomicBool,
    progress_tx: &mpsc::UnboundedSender<CleanupMessage>,
) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    let total = items.len();

    for (i, item) in reversible_first(items).into_iter().enumerate() {
        let path = &item.path;
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        if let CleanupAction::Handler { handler, id } = &item.action {
            match run_handler(handler, id, item, opts) {
                Ok(()) => result.succeeded.push(path.clone()),
                Err(e) => result.failed.push((path.clone(), e)),
            }
        } else if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
        } else if opts.dry_run {
            result.succeeded.push(path.clone());
        } else if !item.ignore_patterns.is_empty() && path.is_dir() {
            match delete_dir_filtered(path, &item.ignore_patterns) {
                Ok(()) => {
                    if opts.audit_log {
                        audit::log_operation("DELETE", path, item.size, &item.module_id);
                    }
                    result.succeeded.push(path.clone());
                }
                Err(e) => result.failed.push((path.clone(), e.to_string())),
            }
        } else {
            let res = if safety::is_symlink(path) {
                std::fs::remove_file(path)
            } else if path.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            };

            match res {
                Ok(()) => {
                    if opts.audit_log {
                        audit::log_operation("DELETE", path, item.size, &item.module_id);
                    }
                    result.succeeded.push(path.clone());
                }
                Err(e) => result.failed.push((path.clone(), e.to_string())),
            }
        }

        let _ = progress_tx.send(CleanupMessage::Progress {
            done: i + 1,
            total,
            path: path.clone(),
            label: item.action.removal_command(),
        });
    }

    result
}

/// Delete a directory's contents while preserving entries matching ignore patterns.
fn delete_dir_filtered(path: &Path, ignore_patterns: &[String]) -> std::io::Result<()> {
    let compiled: Vec<glob::Pattern> = ignore_patterns
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if compiled.iter().any(|p| p.matches(&name_str)) {
            continue;
        }
        let entry_path = entry.path();
        if entry_path.is_dir() {
            std::fs::remove_dir_all(&entry_path)?;
        } else {
            std::fs::remove_file(&entry_path)?;
        }
    }
    Ok(())
}

/// Trash a directory's contents while preserving entries matching ignore patterns.
fn trash_dir_filtered(path: &Path, ignore_patterns: &[String]) -> Result<(), String> {
    let compiled: Vec<glob::Pattern> = ignore_patterns
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if compiled.iter().any(|p| p.matches(&name_str)) {
            continue;
        }
        trash::delete(entry.path()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Run safety checks on a path. Returns an error reason if blocked, or `None` if safe.
fn check_safety(path: &Path, opts: &CleanupOptions) -> Option<String> {
    let (level, reason) = safety::classify_path(path, &opts.protected_paths, opts.enforce_scope);
    match level {
        safety::SafetyLevel::Deny => Some(format!(
            "blocked by safety rule: {}",
            reason.unwrap_or_default()
        )),
        safety::SafetyLevel::Warn if !opts.allow_warned => Some(format!(
            "blocked by safety rule: {}",
            reason.unwrap_or_default()
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // --- handler actions ---

    fn handler_item(id: &str) -> CleanupItem {
        CleanupItem {
            path: crate::core::handlers::identity_path("xcode.simulator-devices", id),
            ignore_patterns: Vec::new(),
            module_id: "xcode-simulators".to_string(),
            size: Some(1024),
            action: CleanupAction::Handler {
                handler: "xcode.simulator-devices",
                id: id.to_string(),
            },
        }
    }

    fn path_item(path: &Path) -> CleanupItem {
        CleanupItem::from(path.to_path_buf())
    }

    #[test]
    fn handler_actions_are_never_reversible() {
        assert!(CleanupAction::Path.is_reversible());
        assert!(!handler_item("ABC").action.is_reversible());
    }

    #[test]
    fn handler_action_exposes_the_exact_command() {
        assert_eq!(
            handler_item("ABC-123").action.removal_command().as_deref(),
            Some("xcrun simctl delete ABC-123")
        );
        assert_eq!(CleanupAction::Path.removal_command(), None);
    }

    /// Reversible work must run first so cancelling partway through leaves the
    /// permanent operations undone.
    #[test]
    fn reversible_work_is_ordered_before_irreversible() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("a.txt");
        let b = tmp.path().join("b.txt");
        std::fs::write(&a, "x").unwrap();
        std::fs::write(&b, "x").unwrap();

        let items = vec![
            handler_item("FIRST"),
            path_item(&a),
            handler_item("SECOND"),
            path_item(&b),
        ];

        let ordered = reversible_first(&items);
        let reversible: Vec<bool> = ordered.iter().map(|i| i.action.is_reversible()).collect();
        assert_eq!(reversible, vec![true, true, false, false]);
    }

    /// A handler item's identity path is synthetic. Dry-run must short-circuit
    /// before the handler runs, and must never touch the filesystem.
    #[test]
    fn dry_run_does_not_invoke_the_handler() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        let opts = CleanupOptions {
            dry_run: true,
            audit_log: false,
            ..Default::default()
        };

        let items = vec![handler_item("ABC")];
        let result = delete_items(&items, &opts, &cancel, &tx);

        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!items[0].path.exists(), "identity path is not a real file");
    }

    #[test]
    fn unknown_handler_fails_the_item_rather_than_the_batch() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);
        let opts = CleanupOptions {
            audit_log: false,
            ..Default::default()
        };

        let items = vec![CleanupItem {
            action: CleanupAction::Handler {
                handler: "gone.missing",
                id: "x".to_string(),
            },
            ..CleanupItem::from(PathBuf::from("freespace-handler/gone.missing/x"))
        }];
        let result = delete_items(&items, &opts, &cancel, &tx);

        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].1.contains("unknown handler"));
    }

    use super::*;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn default_opts() -> CleanupOptions {
        CleanupOptions {
            audit_log: false,
            enforce_scope: false,
            ..CleanupOptions::default()
        }
    }

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    fn test_tx() -> mpsc::UnboundedSender<CleanupMessage> {
        let (tx, _rx) = mpsc::unbounded_channel();
        tx
    }

    /// Convert PathBufs to CleanupItems with no ignore patterns.
    fn items(paths: &[PathBuf]) -> Vec<CleanupItem> {
        paths.iter().map(|p| CleanupItem::from(p.clone())).collect()
    }

    #[test]
    fn delete_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("deleteme.txt");
        fs::write(&file, "data").unwrap();
        assert!(file.exists());

        let result = delete_items(
            &items(std::slice::from_ref(&file)),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn delete_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("deleteme");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("inner.txt"), "data").unwrap();

        let result = delete_items(
            &items(std::slice::from_ref(&dir)),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn delete_nonexistent_fails() {
        let result = delete_items(
            &items(&[PathBuf::from("/nonexistent/path/xyz123")]),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn delete_mixed() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("exists.txt");
        fs::write(&file, "data").unwrap();
        let missing = PathBuf::from("/nonexistent/path/xyz123");

        let result = delete_items(
            &items(&[file.clone(), missing]),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn trash_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("trashme.txt");
        fs::write(&file, "data").unwrap();

        let result = trash_items(
            &items(std::slice::from_ref(&file)),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn trash_nonexistent_fails() {
        let result = trash_items(
            &items(&[PathBuf::from("/nonexistent/path/xyz123")]),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn dry_run_does_not_delete() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("keep.txt");
        fs::write(&file, "data").unwrap();

        let opts = CleanupOptions {
            dry_run: true,
            audit_log: false,
            enforce_scope: false,
            ..CleanupOptions::default()
        };
        let result = delete_items(
            &items(std::slice::from_ref(&file)),
            &opts,
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(file.exists(), "file should still exist in dry-run mode");
    }

    #[test]
    fn dry_run_trash_does_not_delete() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("keep.txt");
        fs::write(&file, "data").unwrap();

        let opts = CleanupOptions {
            dry_run: true,
            audit_log: false,
            enforce_scope: false,
            ..CleanupOptions::default()
        };
        let result = trash_items(
            &items(std::slice::from_ref(&file)),
            &opts,
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(file.exists(), "file should still exist in dry-run mode");
    }

    #[test]
    fn blocked_by_protected_path() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("protected.txt");
        fs::write(&file, "data").unwrap();

        let opts = CleanupOptions {
            protected_paths: vec![tmp.path().to_path_buf()],
            audit_log: false,
            enforce_scope: false,
            ..CleanupOptions::default()
        };
        let result = delete_items(
            &items(std::slice::from_ref(&file)),
            &opts,
            &no_cancel(),
            &test_tx(),
        );
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].1.contains("blocked by safety rule"));
        assert!(file.exists(), "file should not have been deleted");
    }

    #[cfg(unix)]
    #[test]
    fn delete_symlink_dir_removes_link_only() {
        let tmp = TempDir::new().unwrap();
        let target_dir = tmp.path().join("real_dir");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("file.txt"), "data").unwrap();

        let link = tmp.path().join("link_dir");
        std::os::unix::fs::symlink(&target_dir, &link).unwrap();

        let result = delete_items(
            &items(std::slice::from_ref(&link)),
            &default_opts(),
            &no_cancel(),
            &test_tx(),
        );
        assert_eq!(result.succeeded.len(), 1);
        assert!(!link.exists(), "symlink should be removed");
        assert!(target_dir.exists(), "target directory should still exist");
        assert!(
            target_dir.join("file.txt").exists(),
            "target contents should still exist"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn warn_tier_blocked_without_allow() {
        let opts = CleanupOptions {
            audit_log: false,
            enforce_scope: false,
            allow_warned: false,
            ..CleanupOptions::default()
        };
        let result = check_safety(Path::new("/Library/Logs/DiagnosticReports"), &opts);
        assert!(result.is_some());
        assert!(result.unwrap().contains("blocked by safety rule"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn warn_tier_allowed_with_flag() {
        let opts = CleanupOptions {
            audit_log: false,
            enforce_scope: false,
            allow_warned: true,
            ..CleanupOptions::default()
        };
        let result = check_safety(Path::new("/Library/Logs/DiagnosticReports"), &opts);
        assert!(result.is_none());
    }

    #[test]
    fn deny_tier_always_blocked() {
        let opts = CleanupOptions {
            audit_log: false,
            enforce_scope: false,
            allow_warned: true,
            ..CleanupOptions::default()
        };
        let result = check_safety(Path::new("/usr/bin/ls"), &opts);
        assert!(result.is_some());
        assert!(result.unwrap().contains("blocked by safety rule"));
    }

    #[test]
    fn cancel_stops_early() {
        let tmp = TempDir::new().unwrap();
        let f1 = tmp.path().join("a.txt");
        let f2 = tmp.path().join("b.txt");
        let f3 = tmp.path().join("c.txt");
        fs::write(&f1, "a").unwrap();
        fs::write(&f2, "b").unwrap();
        fs::write(&f3, "c").unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Pre-set cancel so it stops before processing any items
        cancel.store(true, Ordering::Relaxed);

        let result = delete_items(
            &items(&[f1.clone(), f2.clone(), f3.clone()]),
            &default_opts(),
            &cancel,
            &tx,
        );

        // Nothing should have been processed
        assert!(result.succeeded.is_empty());
        assert!(result.failed.is_empty());
        assert!(f1.exists());
        assert!(f2.exists());
        assert!(f3.exists());

        // No progress messages sent
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn delete_with_ignore_preserves_matching() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("target_dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("cache.dat"), "cache").unwrap();
        fs::write(dir.join("config.plist"), "config").unwrap();
        fs::write(dir.join("other.log"), "log").unwrap();

        let cleanup_items = vec![CleanupItem {
            path: dir.clone(),
            ignore_patterns: vec!["config.plist".to_string()],
            module_id: String::new(),
            size: None,
            action: CleanupAction::Path,
        }];

        let result = delete_items(&cleanup_items, &default_opts(), &no_cancel(), &test_tx());
        assert_eq!(result.succeeded.len(), 1);
        // The directory and ignored file should still exist
        assert!(dir.exists(), "directory should still exist");
        assert!(
            dir.join("config.plist").exists(),
            "ignored file should be preserved"
        );
        // Other files should be gone
        assert!(!dir.join("cache.dat").exists(), "cache should be deleted");
        assert!(!dir.join("other.log").exists(), "log should be deleted");
    }

    #[test]
    fn delete_with_glob_ignore() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("target_dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("data.dat"), "data").unwrap();
        fs::write(dir.join("a.lock"), "lock1").unwrap();
        fs::write(dir.join("b.lock"), "lock2").unwrap();

        let cleanup_items = vec![CleanupItem {
            path: dir.clone(),
            ignore_patterns: vec!["*.lock".to_string()],
            module_id: String::new(),
            size: None,
            action: CleanupAction::Path,
        }];

        let result = delete_items(&cleanup_items, &default_opts(), &no_cancel(), &test_tx());
        assert_eq!(result.succeeded.len(), 1);
        assert!(!dir.join("data.dat").exists(), "data should be deleted");
        assert!(dir.join("a.lock").exists(), "a.lock should be preserved");
        assert!(dir.join("b.lock").exists(), "b.lock should be preserved");
    }
}
