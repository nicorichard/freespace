// Module installation from GitHub repositories and local paths.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::module::manifest::Module;
use crate::module::source::{SourceFile, SourceIdentifier, SourceInfo};

/// Errors that can occur during module installation.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("git is not installed or not in PATH")]
    GitNotFound,
    #[error("git clone failed: {0}")]
    CloneFailed(String),
    #[error("no modules found in source")]
    NoModulesFound,
    #[error("module '{name}' not found in source. Available: {available}")]
    ModuleNotInRepo { name: String, available: String },
    #[error("failed to parse module.toml in '{path}': {reason}")]
    ManifestParseError { path: String, reason: String },
    #[error("local path does not exist: {0}")]
    PathNotFound(String),
    #[error("user cancelled installation")]
    Cancelled,
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Result of installing a single module.
#[derive(Debug)]
pub struct InstallResult {
    pub name: String,
    pub version: String,
    pub installed_to: PathBuf,
    pub was_upgrade: bool,
}

/// Layout of a source directory.
pub(crate) enum RepoLayout {
    /// module.toml at the root
    SingleModule { module: Module },
    /// Subdirectories each containing module.toml
    MultiModule { modules: Vec<(String, Module)> },
}

/// Install modules from a source identifier string.
///
/// Returns the list of successfully installed modules.
pub fn install(source_str: &str, modules_dir: &Path) -> Result<Vec<InstallResult>, InstallError> {
    let source = SourceIdentifier::parse(source_str)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("{}", e)))?;

    match &source {
        SourceIdentifier::GitHub { .. } => install_from_github(&source, modules_dir),
        SourceIdentifier::Local { path } => install_from_local(&source, path, modules_dir),
    }
}

/// Install from a GitHub repository: clone, install, cleanup.
fn install_from_github(
    source: &SourceIdentifier,
    modules_dir: &Path,
) -> Result<Vec<InstallResult>, InstallError> {
    check_git_available()?;

    let (temp_dir, commit_sha) = clone_repo(source)?;
    let result = install_from_dir(source, &temp_dir, &Some(commit_sha), modules_dir);

    // Best-effort cleanup
    let _ = fs::remove_dir_all(&temp_dir);

    result
}

/// Install from a local directory path by creating a symlink.
///
/// This makes local installs behave like a "dev link" — changes to the source
/// directory are immediately reflected without reinstalling.
fn install_from_local(
    source: &SourceIdentifier,
    path: &Path,
    modules_dir: &Path,
) -> Result<Vec<InstallResult>, InstallError> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| InstallError::Other(anyhow::anyhow!("failed to get cwd: {}", e)))?
            .join(path)
    };

    if !resolved.exists() {
        return Err(InstallError::PathNotFound(resolved.display().to_string()));
    }

    let layout = detect_layout(&resolved)?;

    match layout {
        RepoLayout::SingleModule { module } => {
            let dir_name = source.default_dir_name();
            let dest = modules_dir.join(&dir_name);
            symlink_module(&resolved, &dest)?;
            Ok(vec![InstallResult {
                name: module.name,
                version: module.version,
                installed_to: dest,
                was_upgrade: false,
            }])
        }
        RepoLayout::MultiModule { modules } => {
            if let Some(requested) = source.module_path() {
                let (dir_name, module) = modules
                    .into_iter()
                    .find(|(name, _)| name == requested)
                    .ok_or_else(|| {
                        let available = modules_available_names(&resolved);
                        InstallError::ModuleNotInRepo {
                            name: requested.clone(),
                            available,
                        }
                    })?;

                let src = resolved.join(&dir_name);
                let dest = modules_dir.join(&dir_name);
                symlink_module(&src, &dest)?;
                Ok(vec![InstallResult {
                    name: module.name,
                    version: module.version,
                    installed_to: dest,
                    was_upgrade: false,
                }])
            } else {
                let selected = prompt_module_selection(&modules)?;
                if selected.is_empty() {
                    return Err(InstallError::Cancelled);
                }

                let mut results = Vec::new();
                for idx in selected {
                    let (dir_name, module) = &modules[idx];
                    let src = resolved.join(dir_name);
                    let dest = modules_dir.join(dir_name);
                    symlink_module(&src, &dest)?;
                    results.push(InstallResult {
                        name: module.name.clone(),
                        version: module.version.clone(),
                        installed_to: dest,
                        was_upgrade: false,
                    });
                }
                Ok(results)
            }
        }
    }
}

/// Create a symlink from dest -> src, removing any existing dest first.
fn symlink_module(src: &Path, dest: &Path) -> Result<(), InstallError> {
    if dest.exists() || dest.symlink_metadata().is_ok() {
        if dest.is_dir() && !dest.symlink_metadata().is_ok_and(|m| m.is_symlink()) {
            fs::remove_dir_all(dest)
        } else {
            fs::remove_file(dest)
        }
        .map_err(|e| {
            InstallError::Other(anyhow::anyhow!(
                "failed to remove existing module at {}: {}",
                dest.display(),
                e
            ))
        })?;
    }

    std::os::unix::fs::symlink(src, dest).map_err(|e| {
        InstallError::Other(anyhow::anyhow!(
            "failed to symlink {} -> {}: {}",
            dest.display(),
            src.display(),
            e
        ))
    })
}

/// Core installation logic from a source directory (used for GitHub clones).
fn install_from_dir(
    source: &SourceIdentifier,
    source_dir: &Path,
    commit_sha: &Option<String>,
    modules_dir: &Path,
) -> Result<Vec<InstallResult>, InstallError> {
    let layout = detect_layout(source_dir)?;

    match layout {
        RepoLayout::SingleModule { module } => {
            let source_info = make_source_info(source, commit_sha, None);
            let dir_name = source.default_dir_name();
            let dest = modules_dir.join(&dir_name);
            let result = install_module_dir(source_dir, &dest, &source_info, &module)?;
            Ok(vec![result])
        }
        RepoLayout::MultiModule { modules } => {
            // If a specific module was requested, install just that one
            if let Some(requested) = source.module_path() {
                let (dir_name, module) = modules
                    .into_iter()
                    .find(|(name, _)| name == requested)
                    .ok_or_else(|| {
                        let available = modules_available_names(source_dir);
                        InstallError::ModuleNotInRepo {
                            name: requested.clone(),
                            available,
                        }
                    })?;

                let src = source_dir.join(&dir_name);
                let dest = modules_dir.join(&dir_name);
                let source_info = make_source_info(source, commit_sha, Some(&dir_name));
                let result = install_module_dir(&src, &dest, &source_info, &module)?;
                Ok(vec![result])
            } else {
                // Interactive multi-select
                let selected = prompt_module_selection(&modules)?;
                if selected.is_empty() {
                    return Err(InstallError::Cancelled);
                }

                let mut results = Vec::new();
                for idx in selected {
                    let (dir_name, module) = &modules[idx];
                    let src = source_dir.join(dir_name);
                    let dest = modules_dir.join(dir_name);
                    let source_info = make_source_info(source, commit_sha, Some(dir_name));
                    let result = install_module_dir(&src, &dest, &source_info, module)?;
                    results.push(result);
                }
                Ok(results)
            }
        }
    }
}

/// Helper to collect available module names from a dir (for error messages).
fn modules_available_names(source_dir: &Path) -> String {
    let entries = match fs::read_dir(source_dir) {
        Ok(e) => e,
        Err(_) => return String::from("(unable to list)"),
    };

    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("module.toml").exists() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.join(", ")
}

/// Show an interactive multi-select prompt for choosing modules to install.
fn prompt_module_selection(modules: &[(String, Module)]) -> Result<Vec<usize>, InstallError> {
    crate::tui::views::install_select::run_install_select(modules)
}

/// Check that git is available on the system.
fn check_git_available() -> Result<(), InstallError> {
    Command::new("git")
        .arg("--version")
        .output()
        .map_err(|_| InstallError::GitNotFound)?;
    Ok(())
}

/// Clone a repository to a temporary directory. Returns (temp_dir_path, commit_sha).
fn clone_repo(source: &SourceIdentifier) -> Result<(PathBuf, String), InstallError> {
    let clone_url = source
        .clone_url()
        .ok_or_else(|| InstallError::CloneFailed("not a GitHub source".to_string()))?;

    let dir_name = source.default_dir_name();
    let temp_dir = std::env::temp_dir().join(format!("freespace-install-{}", dir_name));

    // Clean up any previous temp dir
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }

    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");

    if let Some(git_ref) = source.git_ref() {
        cmd.arg("--branch").arg(git_ref);
    }

    cmd.arg(clone_url).arg(&temp_dir);

    let output = cmd
        .output()
        .map_err(|e| InstallError::CloneFailed(format!("failed to run git: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::CloneFailed(stderr.trim().to_string()));
    }

    // Get commit SHA
    let rev_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&temp_dir)
        .output()
        .map_err(|e| InstallError::CloneFailed(format!("failed to get commit SHA: {}", e)))?;

    let commit_sha = String::from_utf8_lossy(&rev_output.stdout)
        .trim()
        .to_string();

    Ok((temp_dir, commit_sha))
}

/// Detect whether a directory is a single-module or multi-module layout.
pub(crate) fn detect_layout(source_dir: &Path) -> Result<RepoLayout, InstallError> {
    // Check for root-level module.toml first
    let root_manifest = source_dir.join("module.toml");
    if root_manifest.exists() {
        let module = parse_manifest(&root_manifest)?;
        return Ok(RepoLayout::SingleModule { module });
    }

    // Check subdirectories for module.toml files
    let mut modules = Vec::new();
    let entries = fs::read_dir(source_dir)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("failed to read source dir: {}", e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip .git directory
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }

        let manifest_path = path.join("module.toml");
        if manifest_path.exists() {
            let module = parse_manifest(&manifest_path)?;
            let dir_name = path.file_name().unwrap().to_string_lossy().to_string();
            modules.push((dir_name, module));
        }
    }

    if modules.is_empty() {
        return Err(InstallError::NoModulesFound);
    }

    // Sort by directory name for stable ordering
    modules.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(RepoLayout::MultiModule { modules })
}

/// Parse a module.toml file.
fn parse_manifest(path: &Path) -> Result<Module, InstallError> {
    let content = fs::read_to_string(path).map_err(|e| InstallError::ManifestParseError {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    Module::parse(&content).map_err(|e| InstallError::ManifestParseError {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

/// Copy a module directory to the install destination and write source.toml.
pub(crate) fn install_module_dir(
    src: &Path,
    dest: &Path,
    source_info: &SourceInfo,
    module: &Module,
) -> Result<InstallResult, InstallError> {
    let was_upgrade = dest.exists();

    // Remove existing module directory if upgrading
    if was_upgrade {
        fs::remove_dir_all(dest).map_err(|e| {
            InstallError::Other(anyhow::anyhow!(
                "failed to remove existing module at {}: {}",
                dest.display(),
                e
            ))
        })?;
    }

    // Copy module files
    copy_dir_recursive(src, dest).map_err(|e| {
        InstallError::Other(anyhow::anyhow!(
            "failed to copy module to {}: {}",
            dest.display(),
            e
        ))
    })?;

    // Write source.toml
    let source_file = SourceFile {
        source: source_info.clone(),
    };
    let source_toml = toml::to_string_pretty(&source_file).map_err(|e| {
        InstallError::Other(anyhow::anyhow!("failed to serialize source.toml: {}", e))
    })?;
    fs::write(dest.join("source.toml"), source_toml)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("failed to write source.toml: {}", e)))?;

    Ok(InstallResult {
        name: module.name.clone(),
        version: module.version.clone(),
        installed_to: dest.to_path_buf(),
        was_upgrade,
    })
}

/// Recursively copy a directory, skipping `.git/`.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();

        // Skip .git directory
        if file_name == ".git" {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&file_name);

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Build a SourceInfo from a source identifier and optional commit metadata.
fn make_source_info(
    source: &SourceIdentifier,
    commit_sha: &Option<String>,
    path: Option<&str>,
) -> SourceInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    SourceInfo {
        repository: source.repository_string(),
        git_ref: source.git_ref().cloned(),
        commit: commit_sha.clone().unwrap_or_default(),
        path: path.map(|s| s.to_string()),
        installed_at: now,
    }
}

/// Read source.toml from an installed module directory, if it exists.
pub fn read_source_info(module_dir: &Path) -> Option<SourceInfo> {
    let path = module_dir.join("source.toml");
    let content = fs::read_to_string(path).ok()?;
    let source_file: SourceFile = toml::from_str(&content).ok()?;
    Some(source_file.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_module_toml(dir: &Path, name: &str) {
        let toml = format!(
            r#"id = "{}"
name = "{}"
version = "1.0.0"
description = "Test"
author = "tester"
platforms = ["macos", "linux"]

[[targets]]
path = "~/test"
"#,
            name, name
        );
        fs::write(dir.join("module.toml"), toml).unwrap();
    }

    // --- detect_layout ---

    #[test]
    fn detect_layout_single_module() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_module_toml(tmp.path(), "single");

        match detect_layout(tmp.path()).unwrap() {
            RepoLayout::SingleModule { module } => {
                assert_eq!(module.name, "single");
            }
            _ => panic!("expected SingleModule"),
        }
    }

    #[test]
    fn detect_layout_multi_module() {
        let tmp = tempfile::TempDir::new().unwrap();
        let alpha = tmp.path().join("alpha");
        fs::create_dir(&alpha).unwrap();
        write_module_toml(&alpha, "alpha-mod");

        let beta = tmp.path().join("beta");
        fs::create_dir(&beta).unwrap();
        write_module_toml(&beta, "beta-mod");

        match detect_layout(tmp.path()).unwrap() {
            RepoLayout::MultiModule { modules } => {
                assert_eq!(modules.len(), 2);
                assert_eq!(modules[0].0, "alpha");
                assert_eq!(modules[1].0, "beta");
            }
            _ => panic!("expected MultiModule"),
        }
    }

    #[test]
    fn detect_layout_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = detect_layout(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn detect_layout_skips_git_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let git_dir = tmp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        write_module_toml(&git_dir, "should-skip");

        let alpha = tmp.path().join("alpha");
        fs::create_dir(&alpha).unwrap();
        write_module_toml(&alpha, "alpha-mod");

        match detect_layout(tmp.path()).unwrap() {
            RepoLayout::MultiModule { modules } => {
                assert_eq!(modules.len(), 1);
                assert_eq!(modules[0].0, "alpha");
            }
            _ => panic!("expected MultiModule"),
        }
    }

    // --- install_module_dir ---

    #[test]
    fn install_module_dir_copies_and_writes_source() {
        let src_tmp = tempfile::TempDir::new().unwrap();
        write_module_toml(src_tmp.path(), "test-install");
        fs::write(src_tmp.path().join("extra.txt"), "extra data").unwrap();

        let dest_tmp = tempfile::TempDir::new().unwrap();
        let dest = dest_tmp.path().join("test-install");

        let source_info = SourceInfo {
            repository: "github:user/repo".to_string(),
            git_ref: Some("v1.0".to_string()),
            commit: "abc123".to_string(),
            path: None,
            installed_at: 1000,
        };

        let module = Module {
            id: "test-install".to_string(),
            name: "test-install".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            tags: vec![],
            targets: vec![],
        };

        let result = install_module_dir(src_tmp.path(), &dest, &source_info, &module).unwrap();
        assert_eq!(result.name, "test-install");
        assert!(!result.was_upgrade);
        assert!(dest.join("module.toml").exists());
        assert!(dest.join("extra.txt").exists());
        assert!(dest.join("source.toml").exists());

        let info = read_source_info(&dest).unwrap();
        assert_eq!(info.repository, "github:user/repo");
        assert_eq!(info.commit, "abc123");
    }

    #[test]
    fn install_module_dir_upgrade_replaces() {
        let src_tmp = tempfile::TempDir::new().unwrap();
        write_module_toml(src_tmp.path(), "upgrade");

        let dest_tmp = tempfile::TempDir::new().unwrap();
        let dest = dest_tmp.path().join("upgrade");

        // Create existing installation
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("old-file.txt"), "old").unwrap();

        let source_info = SourceInfo {
            repository: "github:user/repo".to_string(),
            git_ref: None,
            commit: "def456".to_string(),
            path: None,
            installed_at: 2000,
        };
        let module = Module {
            id: "upgrade".to_string(),
            name: "upgrade".to_string(),
            version: "2.0.0".to_string(),
            description: "Test".to_string(),
            author: "tester".to_string(),
            platforms: vec![],
            tags: vec![],
            targets: vec![],
        };

        let result = install_module_dir(src_tmp.path(), &dest, &source_info, &module).unwrap();
        assert!(result.was_upgrade);
        assert!(!dest.join("old-file.txt").exists()); // old file removed
        assert!(dest.join("module.toml").exists());
    }

    // --- Local install ---

    #[test]
    fn install_local_creates_symlink() {
        let src_tmp = tempfile::TempDir::new().unwrap();
        write_module_toml(src_tmp.path(), "local-test");

        let dest_tmp = tempfile::TempDir::new().unwrap();

        let source = SourceIdentifier::Local {
            path: src_tmp.path().to_path_buf(),
        };
        let results = install_from_local(&source, src_tmp.path(), dest_tmp.path()).unwrap();
        assert_eq!(results.len(), 1);

        // Verify symlink was created
        let installed = &results[0].installed_to;
        assert!(installed.symlink_metadata().unwrap().is_symlink());
    }

    #[test]
    fn install_local_nonexistent_path() {
        let dest_tmp = tempfile::TempDir::new().unwrap();
        let bad_path = Path::new("/nonexistent/module/path/xyz123");
        let source = SourceIdentifier::Local {
            path: bad_path.to_path_buf(),
        };
        let result = install_from_local(&source, bad_path, dest_tmp.path());
        assert!(result.is_err());
    }

    // --- copy_dir_recursive ---

    #[test]
    fn copy_dir_recursive_skips_git() {
        let src = tempfile::TempDir::new().unwrap();
        fs::create_dir(src.path().join(".git")).unwrap();
        fs::write(src.path().join(".git").join("HEAD"), "ref").unwrap();
        fs::write(src.path().join("file.txt"), "data").unwrap();

        let dst = tempfile::TempDir::new().unwrap();
        let dst_path = dst.path().join("copy");
        copy_dir_recursive(src.path(), &dst_path).unwrap();

        assert!(dst_path.join("file.txt").exists());
        assert!(!dst_path.join(".git").exists());
    }

    // --- read_source_info ---

    #[test]
    fn read_source_info_valid() {
        let tmp = tempfile::TempDir::new().unwrap();
        let toml_content = r#"
[source]
repository = "github:user/repo"
commit = "abc123"
installed_at = 1000
"#;
        fs::write(tmp.path().join("source.toml"), toml_content).unwrap();
        let info = read_source_info(tmp.path()).unwrap();
        assert_eq!(info.repository, "github:user/repo");
        assert_eq!(info.commit, "abc123");
    }

    #[test]
    fn read_source_info_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(read_source_info(tmp.path()).is_none());
    }
}
