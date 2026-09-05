// Module lifecycle management: discovery, loading, install, remove.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::ModulesConfig;
use crate::module::catalog;
use crate::module::manifest::Module;

/// Where a loaded module came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleOrigin {
    /// Vendored into the binary from `catalog/`.
    Builtin,
    /// Installed or hand-written under a modules directory.
    User,
}

/// A module plus its provenance.
pub struct LoadedModule {
    pub module: Module,
    /// Path to `module.toml` on disk. `None` for built-in modules, which have
    /// no file to open or edit.
    pub manifest_path: Option<PathBuf>,
    pub origin: ModuleOrigin,
}

/// Outcome of loading every module source.
pub struct LoadResult {
    pub modules: Vec<LoadedModule>,
    pub warnings: Vec<String>,
}

/// Load modules from the built-in catalog and all configured directories.
///
/// Sources, in order: the built-in catalog, the default modules directory
/// (created if missing), then each extra directory from config and CLI flags.
///
/// A user module whose id matches a built-in **replaces** the built-in, so a
/// catalog entry can be overridden locally. The exception is an installed copy
/// from [`CATALOG_REPO`], which duplicates a built-in rather than customising
/// it; there the built-in wins, so a stale copy cannot shadow a fixed one.
pub fn load_all_modules(
    default_dir: Option<PathBuf>,
    extra_dirs: &[String],
    modules_cfg: &ModulesConfig,
) -> LoadResult {
    let mut all_warnings = Vec::new();

    // 1. Built-in catalog
    let mut builtin: Vec<LoadedModule> = Vec::new();
    if modules_cfg.builtin {
        let (modules, warnings) = catalog::load_catalog(&modules_cfg.disabled);
        all_warnings.extend(warnings);
        builtin.extend(modules.into_iter().map(|module| LoadedModule {
            module,
            manifest_path: None,
            origin: ModuleOrigin::Builtin,
        }));
    }

    // 2. User modules: default directory, then extra directories
    let mut user: Vec<(Module, PathBuf)> = Vec::new();

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
            user.extend(modules);
            all_warnings.extend(warnings);
        }
    }

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
        user.extend(modules);
        all_warnings.extend(warnings);
    }

    // 3. Reconcile against the catalog
    let mut overridden_ids = Vec::new();
    let mut kept_user = Vec::new();

    for (module, manifest_path) in user {
        let clashes = builtin.iter().any(|b| b.module.id == module.id);
        if clashes && is_catalog_duplicate(&manifest_path) {
            continue;
        }
        if clashes {
            overridden_ids.push(module.id.clone());
        }
        kept_user.push(LoadedModule {
            module,
            manifest_path: Some(manifest_path),
            origin: ModuleOrigin::User,
        });
    }

    builtin.retain(|b| !overridden_ids.contains(&b.module.id));
    builtin.extend(kept_user);

    LoadResult {
        modules: builtin,
        warnings: all_warnings,
    }
}

/// Whether an installed module is a copy of one the catalog already ships,
/// rather than a deliberate local override.
fn is_catalog_duplicate(manifest_path: &Path) -> bool {
    let Some(module_dir) = manifest_path.parent() else {
        return false;
    };
    crate::module::installer::read_source_info(module_dir)
        .is_some_and(|info| info.repository == crate::config::CATALOG_SOURCE_REPO)
}

/// Expand a leading `~` or `~/` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    crate::core::paths::expand_tilde(path)
}

/// Discover and load built-in modules from the modules/ directory.
///
/// Scans the given directory for subdirectories containing a `module.toml` file,
/// parses each one, and filters out modules for unsupported platforms.
/// Parse errors are collected as warnings rather than failing the entire load.
pub fn load_builtin_modules(modules_dir: &Path) -> (Vec<(Module, PathBuf)>, Vec<String>) {
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
                    modules.push((module, manifest_path));
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
pub(crate) fn current_platform() -> String {
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
        assert_eq!(modules[0].0.name, "test-mod");
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

    /// Write an installed module claiming to come from the repo the catalog
    /// also ships.
    fn write_catalog_duplicate(dir: &Path, id: &str) {
        let module_dir = dir.join(id);
        fs::create_dir_all(&module_dir).unwrap();
        fs::write(
            module_dir.join("module.toml"),
            format!(
                r#"id = "{id}"
name = "installed {id}"
version = "9.9.9"
description = "Test"
author = "tester"
platforms = ["{platform}"]

[[targets]]
path = "~/test"
"#,
                id = id,
                platform = current_platform()
            ),
        )
        .unwrap();
        fs::write(
            module_dir.join("source.toml"),
            format!(
                "[source]\nrepository = \"{}\"\ncommit = \"abc\"\ninstalled_at = 0\n",
                crate::config::CATALOG_SOURCE_REPO
            ),
        )
        .unwrap();
    }

    fn catalog_enabled() -> ModulesConfig {
        ModulesConfig::default()
    }

    #[test]
    fn catalog_loads_without_any_installed_modules() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loaded = load_all_modules(Some(tmp.path().to_path_buf()), &[], &catalog_enabled());
        assert!(
            !loaded.modules.is_empty(),
            "freespace should be usable with nothing installed"
        );
        assert!(loaded
            .modules
            .iter()
            .all(|m| m.origin == ModuleOrigin::Builtin));
    }

    #[test]
    fn builtin_switch_disables_the_catalog() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = ModulesConfig {
            builtin: false,
            disabled: Vec::new(),
        };
        let loaded = load_all_modules(Some(tmp.path().to_path_buf()), &[], &cfg);
        assert!(loaded.modules.is_empty());
    }

    /// An installed copy of a module the catalog ships must not double up with
    /// the built-in, and the built-in must win so a stale copy cannot shadow it.
    #[test]
    fn installed_duplicate_loses_to_the_builtin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (builtins, _) = crate::module::catalog::load_catalog(&[]);
        let id = builtins[0].id.clone();
        write_catalog_duplicate(tmp.path(), &id);

        let loaded = load_all_modules(Some(tmp.path().to_path_buf()), &[], &catalog_enabled());

        let matching: Vec<_> = loaded
            .modules
            .iter()
            .filter(|m| m.module.id == id)
            .collect();
        assert_eq!(matching.len(), 1, "exactly one module should win");
        assert_eq!(matching[0].origin, ModuleOrigin::Builtin);
    }

    /// A hand-written module with no source.toml is a deliberate local override
    /// and must replace the built-in rather than be discarded.
    #[test]
    fn local_module_overrides_a_builtin_of_the_same_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (builtins, _) = crate::module::catalog::load_catalog(&[]);
        let id = builtins[0].id.clone();
        // No source.toml, so this is not a vendored copy.
        write_module_toml(tmp.path(), &id, &[&current_platform()]);

        let loaded = load_all_modules(Some(tmp.path().to_path_buf()), &[], &catalog_enabled());

        let matching: Vec<_> = loaded
            .modules
            .iter()
            .filter(|m| m.module.id == id)
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].origin, ModuleOrigin::User);
    }

    #[test]
    fn load_all_modules_merges_dirs() {
        let tmp1 = tempfile::TempDir::new().unwrap();
        let tmp2 = tempfile::TempDir::new().unwrap();
        let platform = current_platform();
        write_module_toml(tmp1.path(), "mod-a", &[&platform]);
        write_module_toml(tmp2.path(), "mod-b", &[&platform]);

        let extra = vec![tmp2.path().display().to_string()];
        // `builtin: false` keeps the vendored catalog out of the count.
        let cfg = ModulesConfig {
            builtin: false,
            disabled: Vec::new(),
        };
        let loaded = load_all_modules(Some(tmp1.path().to_path_buf()), &extra, &cfg);
        assert_eq!(loaded.modules.len(), 2);
    }

    #[test]
    fn load_all_modules_warns_missing_extra_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let extra = vec!["/nonexistent/module/dir/xyz".to_string()];
        let cfg = ModulesConfig {
            builtin: false,
            disabled: Vec::new(),
        };
        let loaded = load_all_modules(Some(tmp.path().to_path_buf()), &extra, &cfg);
        assert!(!loaded.warnings.is_empty());
    }
}
