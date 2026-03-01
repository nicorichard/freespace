// Module installation from GitHub repositories.

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
    #[error("no modules found in repository")]
    NoModulesFound,
    #[error("module '{name}' not found in repository. Available: {available}")]
    ModuleNotInRepo { name: String, available: String },
    #[error("failed to parse module.toml in '{path}': {reason}")]
    ManifestParseError { path: String, reason: String },
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

/// Layout of a cloned repository.
enum RepoLayout {
    /// module.toml at the repo root
    SingleModule { module: Module },
    /// Subdirectories each containing module.toml
    MultiModule {
        modules: Vec<(String, Module)>,
    },
}

/// Install modules from a source identifier string.
///
/// Returns the list of successfully installed modules.
pub fn install(source_str: &str, modules_dir: &Path) -> Result<Vec<InstallResult>, InstallError> {
    let source = SourceIdentifier::parse(source_str)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("{}", e)))?;

    check_git_available()?;

    let (temp_dir, commit_sha) = clone_repo(&source)?;
    let result = install_from_clone(&source, &temp_dir, &commit_sha, modules_dir);

    // Best-effort cleanup
    let _ = fs::remove_dir_all(&temp_dir);

    result
}

/// Core installation logic after cloning.
fn install_from_clone(
    source: &SourceIdentifier,
    repo_dir: &Path,
    commit_sha: &str,
    modules_dir: &Path,
) -> Result<Vec<InstallResult>, InstallError> {
    let layout = detect_layout(repo_dir)?;

    match layout {
        RepoLayout::SingleModule { module } => {
            let source_info = make_source_info(source, commit_sha, None);
            let dir_name = &module.name;
            let dest = modules_dir.join(dir_name);
            let result = install_module_dir(repo_dir, &dest, &source_info, &module)?;
            Ok(vec![result])
        }
        RepoLayout::MultiModule { modules } => {
            // If a specific module was requested, install just that one
            if let Some(ref requested) = source.module_path {
                let (dir_name, module) = modules
                    .into_iter()
                    .find(|(name, _)| name == requested)
                    .ok_or_else(|| {
                        let available = modules_available_names(repo_dir);
                        InstallError::ModuleNotInRepo {
                            name: requested.clone(),
                            available,
                        }
                    })?;

                let src = repo_dir.join(&dir_name);
                let dest = modules_dir.join(&module.name);
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
                    let src = repo_dir.join(dir_name);
                    let dest = modules_dir.join(&module.name);
                    let source_info = make_source_info(source, commit_sha, Some(dir_name));
                    let result = install_module_dir(&src, &dest, &source_info, module)?;
                    results.push(result);
                }
                Ok(results)
            }
        }
    }
}

/// Helper to collect available module names from a repo dir (for error messages).
fn modules_available_names(repo_dir: &Path) -> String {
    let entries = match fs::read_dir(repo_dir) {
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
    let items: Vec<String> = modules
        .iter()
        .map(|(dir_name, m)| format!("{} - {}", dir_name, m.description))
        .collect();

    let defaults: Vec<bool> = vec![true; items.len()];

    let selections = dialoguer::MultiSelect::new()
        .with_prompt("Select modules to install")
        .items(&items)
        .defaults(&defaults)
        .interact_opt()
        .map_err(|e| InstallError::Other(anyhow::anyhow!("prompt error: {}", e)))?;

    match selections {
        Some(s) => Ok(s),
        None => Err(InstallError::Cancelled),
    }
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
    let temp_dir = std::env::temp_dir().join(format!(
        "freespace-install-{}-{}",
        source.owner,
        source.repo
    ));

    // Clean up any previous temp dir
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }

    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");

    if let Some(ref git_ref) = source.git_ref {
        cmd.arg("--branch").arg(git_ref);
    }

    cmd.arg(source.clone_url()).arg(&temp_dir);

    let output = cmd.output().map_err(|e| {
        InstallError::CloneFailed(format!("failed to run git: {}", e))
    })?;

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

    let commit_sha = String::from_utf8_lossy(&rev_output.stdout).trim().to_string();

    Ok((temp_dir, commit_sha))
}

/// Detect whether a cloned repo is a single-module or multi-module repo.
fn detect_layout(repo_dir: &Path) -> Result<RepoLayout, InstallError> {
    // Check for root-level module.toml first
    let root_manifest = repo_dir.join("module.toml");
    if root_manifest.exists() {
        let module = parse_manifest(&root_manifest)?;
        return Ok(RepoLayout::SingleModule { module });
    }

    // Check subdirectories for module.toml files
    let mut modules = Vec::new();
    let entries = fs::read_dir(repo_dir)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("failed to read repo dir: {}", e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip .git directory
        if path.file_name().map_or(false, |n| n == ".git") {
            continue;
        }

        let manifest_path = path.join("module.toml");
        if manifest_path.exists() {
            let module = parse_manifest(&manifest_path)?;
            let dir_name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
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
    let content = fs::read_to_string(path).map_err(|e| {
        InstallError::ManifestParseError {
            path: path.display().to_string(),
            reason: e.to_string(),
        }
    })?;

    toml::from_str(&content).map_err(|e| InstallError::ManifestParseError {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

/// Copy a module directory to the install destination and write source.toml.
fn install_module_dir(
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
    let source_toml = toml::to_string_pretty(&source_file)
        .map_err(|e| InstallError::Other(anyhow::anyhow!("failed to serialize source.toml: {}", e)))?;
    fs::write(dest.join("source.toml"), source_toml).map_err(|e| {
        InstallError::Other(anyhow::anyhow!("failed to write source.toml: {}", e))
    })?;

    Ok(InstallResult {
        name: module.name.clone(),
        version: module.version.clone(),
        installed_to: dest.to_path_buf(),
        was_upgrade,
    })
}

/// Recursively copy a directory, skipping `.git/`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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

/// Build a SourceInfo from a source identifier and clone metadata.
fn make_source_info(
    source: &SourceIdentifier,
    commit_sha: &str,
    path: Option<&str>,
) -> SourceInfo {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    SourceInfo {
        repository: source.repository_string(),
        git_ref: source.git_ref.clone(),
        commit: commit_sha.to_string(),
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
