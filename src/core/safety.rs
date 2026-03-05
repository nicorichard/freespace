// Path safety validation for cleanup operations.

use std::path::{Path, PathBuf};

/// Safety classification for a filesystem path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyLevel {
    /// Path is safe to operate on.
    Safe,
    /// Path is in a sensitive location — user should be warned but can proceed.
    Warn,
    /// Path is blocked by safety rules — operation must be denied.
    Deny,
}

/// Returns platform-specific system paths that must never be deleted.
fn default_deny_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![];

    #[cfg(target_os = "macos")]
    paths.extend(
        ["/System", "/usr", "/bin", "/sbin", "/etc"]
            .iter()
            .map(PathBuf::from),
    );

    #[cfg(target_os = "linux")]
    paths.extend(
        [
            "/usr", "/bin", "/sbin", "/etc", "/var", "/opt", "/boot", "/lib", "/lib64",
        ]
        .iter()
        .map(PathBuf::from),
    );

    #[cfg(target_os = "windows")]
    paths.extend(
        [
            "C:\\Windows",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
        ]
        .iter()
        .map(PathBuf::from),
    );

    // Home-relative protected directories (all platforms)
    if let Some(home) = dirs::home_dir() {
        paths.extend(
            [
                "Documents",
                "Desktop",
                "Pictures",
                "Music",
                "Movies",
                ".ssh",
                ".gnupg",
            ]
            .iter()
            .map(|d| home.join(d)),
        );
    }

    paths
}

/// Returns platform-specific paths that warrant a warning but are not hard-denied.
fn default_warn_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = vec![];

    #[cfg(target_os = "macos")]
    paths.extend(["/Library", "/Applications"].iter().map(PathBuf::from));

    paths
}

/// Expand `~` prefix to the user's home directory.
fn expand_user_path(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

/// Check if `path` is blocked by the deny-list (builtins + user-configured extras).
/// Returns the matching deny rule as a string if blocked, or `None` if allowed.
pub fn is_path_denied(path: &Path, extra_deny: &[PathBuf]) -> Option<String> {
    // Canonicalize the target path; if we can't resolve it, use as-is.
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Block the filesystem root as a special case (exact match only).
    let root = PathBuf::from("/");
    if canonical == root {
        return Some("/".to_string());
    }

    let mut deny = default_deny_paths();
    deny.extend(extra_deny.iter().cloned());

    for deny_path in &deny {
        let deny_canonical = deny_path
            .canonicalize()
            .unwrap_or_else(|_| deny_path.clone());
        if canonical == deny_canonical || canonical.starts_with(&deny_canonical) {
            return Some(deny_path.display().to_string());
        }
    }

    None
}

/// Classify a path's safety level, returning the level and an optional reason string.
///
/// Checks (in order):
/// 1. Deny list (builtins + user extras) → `Deny`
/// 2. Warn list → `Warn`
/// 3. Outside `$HOME` when `enforce_scope` is true → `Warn`
/// 4. Otherwise → `Safe`
pub fn classify_path(
    path: &Path,
    extra_deny: &[PathBuf],
    enforce_scope: bool,
) -> (SafetyLevel, Option<String>) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Root is always denied
    let root = PathBuf::from("/");
    if canonical == root {
        return (SafetyLevel::Deny, Some("/".to_string()));
    }

    // Check deny list
    let mut deny = default_deny_paths();
    deny.extend(extra_deny.iter().cloned());

    for deny_path in &deny {
        let deny_canonical = deny_path
            .canonicalize()
            .unwrap_or_else(|_| deny_path.clone());
        if canonical == deny_canonical || canonical.starts_with(&deny_canonical) {
            return (SafetyLevel::Deny, Some(deny_path.display().to_string()));
        }
    }

    // Check warn list
    let warn = default_warn_paths();
    for warn_path in &warn {
        let warn_canonical = warn_path
            .canonicalize()
            .unwrap_or_else(|_| warn_path.clone());
        if canonical == warn_canonical || canonical.starts_with(&warn_canonical) {
            return (SafetyLevel::Warn, Some(warn_path.display().to_string()));
        }
    }

    // Outside home scope → warn (not deny)
    if enforce_scope && !is_path_in_scope(path) {
        return (
            SafetyLevel::Warn,
            Some("path is outside home directory".to_string()),
        );
    }

    (SafetyLevel::Safe, None)
}

/// Check if `path` is under the user's home directory.
pub fn is_path_in_scope(path: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let home_canonical = home.canonicalize().unwrap_or(home);
    canonical.starts_with(&home_canonical)
}

/// Reject target patterns that contain `..` path components (directory traversal).
pub fn validate_target_pattern(pattern: &str) -> anyhow::Result<()> {
    for component in Path::new(pattern).components() {
        if matches!(component, std::path::Component::ParentDir) {
            anyhow::bail!(
                "target pattern contains '..': '{}' — directory traversal is not allowed",
                pattern
            );
        }
    }
    Ok(())
}

/// Check if `path` is a symbolic link (not following it).
pub fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Expand user-configured protected path strings (which may contain `~`) into
/// absolute `PathBuf` values suitable for the deny-list.
pub fn expand_protected_paths(configured: &[String]) -> Vec<PathBuf> {
    configured.iter().map(|s| expand_user_path(s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // --- default_deny_paths ---

    #[test]
    fn root_path_denied() {
        // The root "/" is blocked as a special case in is_path_denied.
        assert!(is_path_denied(Path::new("/"), &[]).is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn deny_paths_contains_macos_system() {
        let paths = default_deny_paths();
        assert!(paths.contains(&PathBuf::from("/System")));
        // /Applications and /Library are warn-tier, not deny
        assert!(!paths.contains(&PathBuf::from("/Applications")));
        assert!(!paths.contains(&PathBuf::from("/Library")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn warn_paths_contains_macos_sensitive() {
        let warn = default_warn_paths();
        assert!(warn.contains(&PathBuf::from("/Library")));
        assert!(warn.contains(&PathBuf::from("/Applications")));
    }

    #[test]
    fn deny_paths_contains_home_sensitive() {
        let paths = default_deny_paths();
        if let Some(home) = dirs::home_dir() {
            assert!(paths.contains(&home.join(".ssh")));
            assert!(paths.contains(&home.join("Documents")));
        }
    }

    // --- is_path_denied ---

    #[test]
    fn denied_root() {
        assert!(is_path_denied(Path::new("/"), &[]).is_some());
    }

    #[test]
    fn denied_system_child() {
        // A path under a system dir should be denied
        assert!(is_path_denied(Path::new("/usr/bin/ls"), &[]).is_some());
    }

    #[test]
    fn denied_extra_user_path() {
        let tmp = TempDir::new().unwrap();
        let protected = tmp.path().to_path_buf();
        let child = tmp.path().join("subdir");
        fs::create_dir(&child).unwrap();

        assert!(is_path_denied(&child, &[protected]).is_some());
    }

    #[test]
    fn allowed_temp_path() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("safe.txt");
        fs::write(&file, "ok").unwrap();

        // Temp paths are not in any system deny-list, so they should be allowed.
        assert!(is_path_denied(&file, &[]).is_none());
    }

    // --- is_path_in_scope ---

    #[test]
    fn in_scope_home_subdir() {
        if let Some(home) = dirs::home_dir() {
            let test_path = home.join("some_cache_dir");
            assert!(is_path_in_scope(&test_path));
        }
    }

    #[test]
    fn out_of_scope_system_path() {
        assert!(!is_path_in_scope(Path::new("/usr/local/bin")));
    }

    // --- validate_target_pattern ---

    #[test]
    fn valid_pattern_normal() {
        assert!(validate_target_pattern("~/Library/Caches/test").is_ok());
    }

    #[test]
    fn valid_pattern_glob() {
        assert!(validate_target_pattern("**/node_modules").is_ok());
    }

    #[test]
    fn invalid_pattern_traversal() {
        assert!(validate_target_pattern("~/Library/../../../etc/passwd").is_err());
    }

    #[test]
    fn invalid_pattern_traversal_mid() {
        assert!(validate_target_pattern("/home/user/../root").is_err());
    }

    // --- is_symlink ---

    #[test]
    fn regular_file_not_symlink() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("regular.txt");
        fs::write(&file, "data").unwrap();
        assert!(!is_symlink(&file));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_detected() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("target.txt");
        fs::write(&target, "data").unwrap();
        let link = tmp.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(is_symlink(&link));
    }

    #[test]
    fn nonexistent_not_symlink() {
        assert!(!is_symlink(Path::new("/nonexistent/xyz")));
    }

    // --- expand_protected_paths ---

    #[test]
    fn expand_tilde_path() {
        let result = expand_protected_paths(&["~/Work".to_string()]);
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result[0], home.join("Work"));
        }
    }

    #[test]
    fn expand_absolute_path() {
        let result = expand_protected_paths(&["/opt/data".to_string()]);
        assert_eq!(result[0], PathBuf::from("/opt/data"));
    }

    // --- classify_path ---

    #[test]
    fn classify_system_path_denied() {
        let (level, reason) = classify_path(Path::new("/usr/bin/ls"), &[], false);
        assert_eq!(level, SafetyLevel::Deny);
        assert!(reason.is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn classify_library_path_warned() {
        let (level, reason) = classify_path(Path::new("/Library/Logs/foo"), &[], false);
        assert_eq!(level, SafetyLevel::Warn);
        assert!(reason.unwrap().contains("/Library"));
    }

    #[test]
    fn classify_home_cache_safe() {
        if let Some(home) = dirs::home_dir() {
            let (level, _) = classify_path(&home.join("Library/Caches/foo"), &[], false);
            assert_eq!(level, SafetyLevel::Safe);
        }
    }

    #[test]
    fn classify_outside_home_warns_with_scope() {
        let (level, reason) = classify_path(Path::new("/tmp/some_random_dir"), &[], true);
        // /tmp might resolve to a home-relative path on macOS; test the logic path
        // If it's outside home, it should warn
        if !is_path_in_scope(Path::new("/tmp/some_random_dir")) {
            assert_eq!(level, SafetyLevel::Warn);
            assert!(reason.unwrap().contains("outside home"));
        }
    }

    #[test]
    fn classify_outside_home_safe_without_scope() {
        // With enforce_scope=false, paths outside home are not warned
        // (unless they hit deny/warn lists)
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("safe.txt");
        fs::write(&file, "ok").unwrap();
        let (level, _) = classify_path(&file, &[], false);
        assert_eq!(level, SafetyLevel::Safe);
    }
}
