// File and directory cleanup operations.

use std::path::{Path, PathBuf};

use crate::core::audit;
use crate::core::safety;

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
pub fn trash_items(paths: &[PathBuf], opts: &CleanupOptions) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for path in paths {
        if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
            continue;
        }

        if opts.dry_run {
            result.succeeded.push(path.clone());
            continue;
        }

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

    result
}

/// Delete the given files and directories, returning which succeeded and which failed.
pub fn delete_items(paths: &[PathBuf], opts: &CleanupOptions) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for path in paths {
        if let Some(reason) = check_safety(path, opts) {
            result.failed.push((path.clone(), reason));
            continue;
        }

        if opts.dry_run {
            result.succeeded.push(path.clone());
            continue;
        }

        let res = if safety::is_symlink(path) {
            // Remove just the symlink itself (unlink), never follow into target
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
    use tempfile::TempDir;

    fn default_opts() -> CleanupOptions {
        CleanupOptions {
            audit_log: false,
            enforce_scope: false,
            ..CleanupOptions::default()
        }
    }

    #[test]
    fn delete_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("deleteme.txt");
        fs::write(&file, "data").unwrap();
        assert!(file.exists());

        let result = delete_items(&[file.clone()], &default_opts());
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

        let result = delete_items(&[dir.clone()], &default_opts());
        assert_eq!(result.succeeded.len(), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn delete_nonexistent_fails() {
        let result = delete_items(
            &[PathBuf::from("/nonexistent/path/xyz123")],
            &default_opts(),
        );
        // Should be blocked by safety (outside home) or by nonexistence
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn delete_mixed() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("exists.txt");
        fs::write(&file, "data").unwrap();
        let missing = PathBuf::from("/nonexistent/path/xyz123");

        let result = delete_items(&[file.clone(), missing], &default_opts());
        // file succeeds, missing fails (blocked by safety or not found)
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn trash_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("trashme.txt");
        fs::write(&file, "data").unwrap();

        let result = trash_items(&[file.clone()], &default_opts());
        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn trash_nonexistent_fails() {
        let result = trash_items(
            &[PathBuf::from("/nonexistent/path/xyz123")],
            &default_opts(),
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
        let result = delete_items(&[file.clone()], &opts);
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
        let result = trash_items(&[file.clone()], &opts);
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
        let result = delete_items(&[file.clone()], &opts);
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

        let result = delete_items(&[link.clone()], &default_opts());
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
        // /Library paths are warn-tier; without allow_warned they should be blocked
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
        // /Library paths should pass through when allow_warned is true
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
        // /usr paths are deny-tier regardless of allow_warned
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
}
