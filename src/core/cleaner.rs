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
    },
    /// The entire cleanup operation has finished (or was cancelled).
    Complete(CleanupResult),
}

/// Options controlling cleanup behavior.
pub struct CleanupOptions {
    pub dry_run: bool,
    pub protected_paths: Vec<PathBuf>,
    pub module_id: String,
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
            module_id: String::new(),
            audit_log: true,
            enforce_scope: true,
            allow_warned: false,
        }
    }
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
    paths: &[PathBuf],
    opts: &CleanupOptions,
    cancel: &AtomicBool,
    progress_tx: &mpsc::UnboundedSender<CleanupMessage>,
) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    let total = paths.len();

    for (i, path) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
        } else if opts.dry_run {
            result.succeeded.push(path.clone());
        } else {
            match trash::delete(path) {
                Ok(()) => {
                    if opts.audit_log {
                        audit::log_operation("TRASH", path, None, &opts.module_id);
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
        });
    }

    result
}

/// Delete the given files and directories, returning which succeeded and which failed.
///
/// Checks `cancel` between each item and stops early if set.
/// Sends `CleanupMessage::Progress` after each item and `CleanupMessage::Complete` when done.
pub fn delete_items(
    paths: &[PathBuf],
    opts: &CleanupOptions,
    cancel: &AtomicBool,
    progress_tx: &mpsc::UnboundedSender<CleanupMessage>,
) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    let total = paths.len();

    for (i, path) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
        } else if opts.dry_run {
            result.succeeded.push(path.clone());
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
                        audit::log_operation("DELETE", path, None, &opts.module_id);
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
        });
    }

    result
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

    #[test]
    fn delete_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("deleteme.txt");
        fs::write(&file, "data").unwrap();
        assert!(file.exists());

        let result = delete_items(&[file.clone()], &default_opts(), &no_cancel(), &test_tx());
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

        let result = delete_items(&[dir.clone()], &default_opts(), &no_cancel(), &test_tx());
        assert_eq!(result.succeeded.len(), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn delete_nonexistent_fails() {
        let result = delete_items(
            &[PathBuf::from("/nonexistent/path/xyz123")],
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
            &[file.clone(), missing],
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

        let result = trash_items(&[file.clone()], &default_opts(), &no_cancel(), &test_tx());
        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn trash_nonexistent_fails() {
        let result = trash_items(
            &[PathBuf::from("/nonexistent/path/xyz123")],
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
        let result = delete_items(&[file.clone()], &opts, &no_cancel(), &test_tx());
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
        let result = trash_items(&[file.clone()], &opts, &no_cancel(), &test_tx());
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
        let result = delete_items(&[file.clone()], &opts, &no_cancel(), &test_tx());
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

        let result = delete_items(&[link.clone()], &default_opts(), &no_cancel(), &test_tx());
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
            &[f1.clone(), f2.clone(), f3.clone()],
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
}
