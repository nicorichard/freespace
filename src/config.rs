// Application configuration.

use std::path::{Path, PathBuf};

/// Upstream repository the built-in catalog is sourced from. Used to recognise
/// an installed module as a duplicate of a built-in rather than a deliberate
/// local override.
pub const CATALOG_SOURCE_REPO: &str = "github:nicorichard/freespace-modules";

use serde::{Deserialize, Serialize};

/// Errors that can occur when loading the config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("could not parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("could not serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),
}

/// Icon display settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct IconsConfig {
    pub enabled: bool,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Settings for the built-in module catalog vendored into the binary.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ModulesConfig {
    /// Master switch for the vendored catalog. When false, only modules
    /// installed under `~/.config/freespace/modules/` (and `module_dirs`) load.
    pub builtin: bool,
    /// Ids of vendored modules to skip.
    pub disabled: Vec<String>,
}

impl Default for ModulesConfig {
    fn default() -> Self {
        Self {
            builtin: true,
            disabled: Vec::new(),
        }
    }
}

impl ModulesConfig {
    /// Mark a built-in module id as disabled. Returns false if already disabled.
    pub fn disable(&mut self, id: &str) -> bool {
        if self.disabled.iter().any(|d| d == id) {
            return false;
        }
        self.disabled.push(id.to_string());
        self.disabled.sort();
        true
    }

    /// Re-enable a built-in module id. Returns false if it was not disabled.
    pub fn enable(&mut self, id: &str) -> bool {
        let len = self.disabled.len();
        self.disabled.retain(|d| d != id);
        self.disabled.len() < len
    }

    /// Whether a vendored module id is disabled by the user.
    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.iter().any(|d| d == id)
    }
}

/// Application-level configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(skip)]
    pub dry_run: bool,
    pub module_dirs: Vec<String>,
    pub search_dirs: Vec<String>,
    pub audit_log: bool,
    pub protected_paths: Vec<String>,
    pub enforce_scope: bool,
    pub icons: IconsConfig,
    pub modules: ModulesConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dry_run: true,
            module_dirs: Vec::new(),
            search_dirs: Vec::new(),
            audit_log: true,
            protected_paths: Vec::new(),
            enforce_scope: true,
            icons: IconsConfig::default(),
            modules: ModulesConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from `~/.config/freespace/config.toml`.
    /// Returns defaults if the file doesn't exist.
    pub fn load() -> Result<Self, ConfigError> {
        let path = match config_path() {
            Some(p) => p,
            None => return Ok(Self::default()),
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to `~/.config/freespace/config.toml`.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path().ok_or_else(|| {
            ConfigError::ReadError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine config path",
            ))
        })?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Add a search directory. Returns false if already present.
    pub fn add_search_dir(&mut self, path: String) -> bool {
        if self.search_dirs.contains(&path) {
            return false;
        }
        self.search_dirs.push(path);
        true
    }

    /// Remove a search directory. Returns false if not found.
    pub fn remove_search_dir(&mut self, path: &str) -> bool {
        let len = self.search_dirs.len();
        self.search_dirs.retain(|d| d != path);
        self.search_dirs.len() < len
    }
}

/// Folder names under `$HOME` that commonly hold checked-out projects. Offered
/// as pre-filled choices when a user has no `search_dirs` yet, so setting up
/// project cleanup is a keypress rather than a path to type.
const COMMON_PROJECT_DIRS: &[&str] = &[
    "Developer",
    "Projects",
    "projects",
    "Code",
    "code",
    "src",
    "dev",
    "repos",
    "git",
    "workspace",
    "Documents/GitHub",
];

/// One choice in the project-folder picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDirCandidate {
    /// The path as it is stored in config and shown to the user (`~`-contracted).
    pub path: String,
    /// Whether it is currently chosen.
    pub checked: bool,
    /// Where this choice came from, shown beside the path.
    pub note: Option<&'static str>,
}

/// Build the choices offered by the project-folder picker.
///
/// Already-configured directories come first and stay checked, so the picker
/// doubles as the way to remove one. Everything after them is a guess: common
/// project folders that exist, then the directory freespace was launched from.
pub fn project_dir_candidates(
    home: &Path,
    cwd: Option<&Path>,
    configured: &[String],
) -> Vec<ProjectDirCandidate> {
    let mut candidates: Vec<ProjectDirCandidate> = Vec::new();
    // Identity for de-duplication: the resolved path where it resolves, so a
    // case-insensitive filesystem does not offer ~/Projects and ~/projects as
    // two separate folders.
    let mut seen: Vec<PathBuf> = Vec::new();

    for dir in configured {
        let expanded = crate::core::paths::expand_tilde_in(dir, home);
        let key = std::fs::canonicalize(&expanded).unwrap_or_else(|_| expanded.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        candidates.push(ProjectDirCandidate {
            path: crate::core::paths::contract_tilde(&expanded, home),
            checked: true,
            note: Some("in use"),
        });
    }

    let mut offer = |path: PathBuf, note: Option<&'static str>| {
        if !path.is_dir() {
            return;
        }
        let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&resolved) {
            return;
        }
        // On a case-insensitive filesystem `~/Code` opens a folder named
        // `code`. Offer the name as it is actually spelled, so what lands in
        // config still works on a case-sensitive one.
        let path = match (resolved.file_name(), path.file_name()) {
            (Some(actual), Some(asked)) if actual != asked => path.with_file_name(actual),
            _ => path.clone(),
        };
        seen.push(resolved);
        candidates.push(ProjectDirCandidate {
            path: crate::core::paths::contract_tilde(&path, home),
            checked: false,
            note,
        });
    };

    for name in COMMON_PROJECT_DIRS {
        offer(home.join(name), None);
    }

    // The launch directory is only a useful guess when it is somewhere below
    // home — `~` and `/` are too broad to walk, and offering them invites a
    // scan of the whole disk.
    if let Some(cwd) = cwd {
        if cwd != home && cwd.starts_with(home) {
            offer(cwd.to_path_buf(), Some("current directory"));
        }
    }

    candidates
}

/// Returns `~/.config/freespace` (always uses `~/.config`, not the platform default).
pub fn config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config").join("freespace"))
}

/// Returns `~/.config/freespace/modules`.
pub fn default_modules_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("modules"))
}

/// Returns `~/.config/freespace/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = AppConfig::default();
        assert!(config.dry_run);
        assert!(config.module_dirs.is_empty());
        assert!(config.search_dirs.is_empty());
        assert!(config.audit_log);
        assert!(config.protected_paths.is_empty());
    }

    #[test]
    fn parse_valid_config() {
        let toml_str = r#"
        module_dirs = ["~/extra-modules"]
        search_dirs = ["~/Projects", "~/Work"]
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.module_dirs, vec!["~/extra-modules"]);
        assert_eq!(config.search_dirs, vec!["~/Projects", "~/Work"]);
    }

    #[test]
    fn parse_config_with_safety_fields() {
        let toml_str = r#"
        audit_log = false
        protected_paths = ["~/Work", "~/important-project"]
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.audit_log);
        assert_eq!(
            config.protected_paths,
            vec!["~/Work", "~/important-project"]
        );
    }

    #[test]
    fn parse_empty_config() {
        let config: AppConfig = toml::from_str("").unwrap();
        assert!(config.module_dirs.is_empty());
        assert!(config.search_dirs.is_empty());
        assert!(config.audit_log);
        assert!(config.protected_paths.is_empty());
        assert!(config.icons.enabled);
    }

    #[test]
    fn parse_icons_config() {
        let toml_str = r#"
        [icons]
        enabled = false
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.icons.enabled);
    }

    #[test]
    fn parse_icons_defaults_to_enabled() {
        let config: AppConfig = toml::from_str("").unwrap();
        assert!(config.icons.enabled);
    }

    #[test]
    fn candidates_list_configured_dirs_first_and_checked() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("Projects")).unwrap();

        let candidates = project_dir_candidates(
            home.path(),
            None,
            &["~/Work".to_string(), "~/Projects".to_string()],
        );

        assert_eq!(candidates[0].path, "~/Work");
        assert_eq!(candidates[1].path, "~/Projects");
        assert!(candidates[0].checked && candidates[1].checked);
        // ~/Projects exists as a common dir too, but is only offered once.
        assert_eq!(
            candidates.iter().filter(|c| c.path == "~/Projects").count(),
            1
        );
    }

    #[test]
    fn candidates_offer_only_common_dirs_that_exist() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("Developer")).unwrap();

        let candidates = project_dir_candidates(home.path(), None, &[]);

        let paths: Vec<&str> = candidates.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, vec!["~/Developer"]);
        assert!(!candidates[0].checked);
    }

    #[test]
    fn candidates_offer_a_folder_once_however_it_is_spelled() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("Projects")).unwrap();

        let candidates = project_dir_candidates(home.path(), None, &[]);

        // On a case-insensitive filesystem `~/projects` resolves to the same
        // folder, and is offered once, under the name it really has.
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "~/Projects");
    }

    #[test]
    fn candidates_use_the_spelling_on_disk() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join("code")).unwrap();

        let candidates = project_dir_candidates(home.path(), None, &[]);

        // `Code` is tried before `code`; a case-insensitive filesystem matches
        // it, and the folder is still offered as `~/code`.
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "~/code");
    }

    #[test]
    fn candidates_offer_the_launch_directory() {
        let home = tempfile::tempdir().unwrap();
        let cwd = home.path().join("rust/freespace");
        std::fs::create_dir_all(&cwd).unwrap();

        let candidates = project_dir_candidates(home.path(), Some(&cwd), &[]);

        assert_eq!(candidates[0].path, "~/rust/freespace");
        assert_eq!(candidates[0].note, Some("current directory"));
    }

    #[test]
    fn candidates_skip_a_launch_directory_outside_home() {
        let home = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();

        let candidates = project_dir_candidates(home.path(), Some(elsewhere.path()), &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn candidates_skip_home_itself() {
        let home = tempfile::tempdir().unwrap();

        let candidates = project_dir_candidates(home.path(), Some(home.path()), &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn config_dir_path() {
        let dir = config_dir();
        // Should succeed on any system with a home directory
        if let Some(dir) = dir {
            assert!(dir.ends_with(".config/freespace"));
        }
    }

    #[test]
    fn default_modules_dir_path() {
        let dir = default_modules_dir();
        if let Some(dir) = dir {
            assert!(dir.ends_with("modules"));
        }
    }
}
