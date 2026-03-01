// File and directory cleanup operations.

use std::path::PathBuf;

/// Result of a cleanup operation.
pub struct CleanupResult {
    pub succeeded: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

/// Move the given files and directories to the system trash, returning which succeeded and which failed.
pub fn trash_items(paths: &[PathBuf]) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for path in paths {
        match trash::delete(path) {
            Ok(()) => result.succeeded.push(path.clone()),
            Err(e) => result.failed.push((path.clone(), e.to_string())),
        }
    }

    result
}

/// Delete the given files and directories, returning which succeeded and which failed.
pub fn delete_items(paths: &[PathBuf]) -> CleanupResult {
    let mut result = CleanupResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for path in paths {
        let res = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };

        match res {
            Ok(()) => result.succeeded.push(path.clone()),
            Err(e) => result.failed.push((path.clone(), e.to_string())),
        }
    }

    result
}
