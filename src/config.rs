// Application configuration.

use std::path::PathBuf;

use serde::Deserialize;

/// Errors that can occur when loading the config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("could not parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
}

/// Application-level configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(skip)]
    pub dry_run: bool,
    pub module_dirs: Vec<String>,
    pub search_dirs: Vec<String>,
    pub audit_log: bool,
    pub protected_paths: Vec<String>,
    pub enforce_scope: bool,
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
