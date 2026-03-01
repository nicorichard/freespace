// Module lifecycle management: discovery, loading, install, remove.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::module::manifest::Module;

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
    let module: Module = toml::from_str(&content)?;
    Ok(module)
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

/// Find the built-in modules directory relative to the executable or cwd.
pub fn find_modules_dir() -> Option<PathBuf> {
    // First, try relative to current working directory (development mode)
    let cwd_modules = PathBuf::from("modules");
    if cwd_modules.is_dir() {
        return Some(cwd_modules);
    }

    // Try relative to the executable location
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_modules = exe_dir.join("modules");
            if exe_modules.is_dir() {
                return Some(exe_modules);
            }
        }
    }

    None
}
