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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn delete_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("deleteme.txt");
        fs::write(&file, "data").unwrap();
        assert!(file.exists());

        let result = delete_items(&[file.clone()]);
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

        let result = delete_items(&[dir.clone()]);
        assert_eq!(result.succeeded.len(), 1);
        assert!(!dir.exists());
    }

    #[test]
    fn delete_nonexistent_fails() {
        let result = delete_items(&[PathBuf::from("/nonexistent/path/xyz123")]);
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn delete_mixed() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("exists.txt");
        fs::write(&file, "data").unwrap();
        let missing = PathBuf::from("/nonexistent/path/xyz123");

        let result = delete_items(&[file.clone(), missing]);
        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn trash_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("trashme.txt");
        fs::write(&file, "data").unwrap();

        let result = trash_items(&[file.clone()]);
        assert_eq!(result.succeeded.len(), 1);
        assert!(result.failed.is_empty());
        assert!(!file.exists());
    }

    #[test]
    fn trash_nonexistent_fails() {
        let result = trash_items(&[PathBuf::from("/nonexistent/path/xyz123")]);
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
    }
}
