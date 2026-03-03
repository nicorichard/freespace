// Module lifecycle management: discovery, loading, install, remove.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::module::manifest::Module;

/// Load modules from all configured directories.
///
/// Scans the default modules directory first (creating it if it doesn't exist),
/// then scans each extra directory from config and CLI flags.
/// Returns loaded modules and any warnings encountered.
pub fn load_all_modules(
    default_dir: Option<PathBuf>,
    extra_dirs: &[String],
) -> (Vec<Module>, Vec<String>) {
    let mut all_modules = Vec::new();
    let mut all_warnings = Vec::new();

    // 1. Scan default directory (~/.config/freespace/modules/)
    if let Some(dir) = default_dir {
        if !dir.exists() {
            if let Err(e) = fs::create_dir_all(&dir) {
                all_warnings.push(format!(
                    "Could not create default modules directory {}: {}",
                    dir.display(),
                    e
                ));
            }
        }

        if dir.is_dir() {
            let (modules, warnings) = load_builtin_modules(&dir);
            all_modules.extend(modules);
            all_warnings.extend(warnings);
        }
    }

    // 2. Scan extra directories (from config + CLI)
    for dir_str in extra_dirs {
        let dir = expand_tilde(dir_str);
        if !dir.is_dir() {
            all_warnings.push(format!(
                "Module directory does not exist: {}",
                dir.display()
            ));
            continue;
        }
        let (modules, warnings) = load_builtin_modules(&dir);
        all_modules.extend(modules);
        all_warnings.extend(warnings);
    }

    (all_modules, all_warnings)
}

/// Expand a leading `~` or `~/` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    } else if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Discover and load built-in modules from the modules/ directory.
///
/// Scans the given directory for subdirectories containing a `module.toml` file,
/// parses each one, and filters out modules for unsupported platforms.
/// Parse errors are collected as warnings rather than failing the entire load.
pub fn load_builtin_modules(modules_dir: &Path) -> (Vec<Module>, Vec<String>) {
    let mut modules = Vec::new();
    let mut warnings = Vec::new();

    let entries = match fs::read_dir(modules_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(format!(
                "Could not read modules directory {}: {}",
                modules_dir.display(),
                e
            ));
            return (modules, warnings);
        }
    };

    let current_platform = current_platform();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warnings.push(format!("Error reading directory entry: {}", e));
                continue;
            }
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("module.toml");
        if !manifest_path.exists() {
            continue;
        }

        match load_module(&manifest_path) {
            Ok(module) => {
                if module.platforms.iter().any(|p| p == &current_platform) {
                    modules.push(module);
                }
            }
            Err(e) => {
                warnings.push(format!(
                    "Failed to load module from {}: {}",
                    manifest_path.display(),
                    e
                ));
            }
        }
    }

    (modules, warnings)
}

/// Parse a single module.toml file into a Module struct.
fn load_module(path: &Path) -> anyhow::Result<Module> {
    let content = fs::read_to_string(path)?;
    Module::parse(&content)
}

/// Return the current platform string matching module manifest conventions.
fn current_platform() -> String {
    match env::consts::OS {
        "macos" => "macos".to_string(),
        "linux" => "linux".to_string(),
        "windows" => "windows".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_module_toml(dir: &Path, name: &str, platforms: &[&str]) {
        let module_dir = dir.join(name);
        fs::create_dir_all(&module_dir).unwrap();
        let platforms_str: Vec<String> = platforms.iter().map(|p| format!("\"{}\"", p)).collect();
        let toml = format!(
            r#"id = "{}"
name = "{}"
version = "1.0.0"
description = "Test"
author = "tester"
platforms = [{}]

[[targets]]
path = "~/test"
"#,
            name,
            name,
            platforms_str.join(", ")
        );
        fs::write(module_dir.join("module.toml"), toml).unwrap();
    }

    #[test]
    fn load_builtin_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (modules, warnings) = load_builtin_modules(tmp.path());
        assert!(modules.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn load_builtin_single_matching_platform() {
        let tmp = tempfile::TempDir::new().unwrap();
        let platform = current_platform();
        write_module_toml(tmp.path(), "test-mod", &[&platform]);

        let (modules, warnings) = load_builtin_modules(tmp.path());
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "test-mod");
        assert!(warnings.is_empty());
    }

    #[test]
    fn load_builtin_filtered_by_platform() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_module_toml(tmp.path(), "wrong-platform", &["nonexistent-os"]);

        let (modules, warnings) = load_builtin_modules(tmp.path());
        assert!(modules.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn load_builtin_invalid_manifest() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mod_dir = tmp.path().join("broken");
        fs::create_dir(&mod_dir).unwrap();
        fs::write(mod_dir.join("module.toml"), "invalid toml {{{{").unwrap();

        let (modules, warnings) = load_builtin_modules(tmp.path());
        assert!(modules.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn load_builtin_skips_non_directories() {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::write(tmp.path().join("not-a-dir.toml"), "data").unwrap();

        let (modules, warnings) = load_builtin_modules(tmp.path());
        assert!(modules.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn load_all_modules_merges_dirs() {
        let tmp1 = tempfile::TempDir::new().unwrap();
        let tmp2 = tempfile::TempDir::new().unwrap();
        let platform = current_platform();
        write_module_toml(tmp1.path(), "mod-a", &[&platform]);
        write_module_toml(tmp2.path(), "mod-b", &[&platform]);

        let extra = vec![tmp2.path().display().to_string()];
        let (modules, _) = load_all_modules(Some(tmp1.path().to_path_buf()), &extra);
        assert_eq!(modules.len(), 2);
    }

    #[test]
    fn load_all_modules_warns_missing_extra_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let extra = vec!["/nonexistent/module/dir/xyz".to_string()];
        let (_, warnings) = load_all_modules(Some(tmp.path().to_path_buf()), &extra);
        assert!(!warnings.is_empty());
    }
}
