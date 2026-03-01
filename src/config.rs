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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dry_run: true,
            module_dirs: Vec::new(),
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
