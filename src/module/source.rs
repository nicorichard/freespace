// Source identifier parsing and provenance tracking for installed modules.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Errors that can occur when parsing a source identifier.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("source must start with 'github:' prefix")]
    MissingPrefix,
    #[error("invalid format: expected github:owner/repo[@ref][#module]")]
    InvalidFormat,
}

/// A parsed source identifier like `github:user/repo@v1.0.0#module-name`.
#[derive(Debug, Clone)]
pub struct SourceIdentifier {
    pub owner: String,
    pub repo: String,
    pub git_ref: Option<String>,
    pub module_path: Option<String>,
}

impl SourceIdentifier {
    /// Parse a source string like `github:user/repo@v1.0.0#module-name`.
    pub fn parse(s: &str) -> Result<Self, SourceError> {
        let rest = s.strip_prefix("github:").ok_or(SourceError::MissingPrefix)?;

        // Split on '#' to extract optional module path
        let (repo_part, module_path) = match rest.split_once('#') {
            Some((repo, module)) => {
                if module.is_empty() {
                    return Err(SourceError::InvalidFormat);
                }
                (repo, Some(module.to_string()))
            }
            None => (rest, None),
        };

        // Split on '@' to extract optional git ref
        let (owner_repo, git_ref) = match repo_part.split_once('@') {
            Some((or, r)) => {
                if r.is_empty() {
                    return Err(SourceError::InvalidFormat);
                }
                (or, Some(r.to_string()))
            }
            None => (repo_part, None),
        };

        // Split owner/repo
        let (owner, repo) = owner_repo
            .split_once('/')
            .ok_or(SourceError::InvalidFormat)?;

        if owner.is_empty() || repo.is_empty() {
            return Err(SourceError::InvalidFormat);
        }

        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref,
            module_path,
        })
    }

    /// HTTPS clone URL for the repository.
    pub fn clone_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.repo)
    }

    /// The `github:owner/repo` string (without ref or module path).
    pub fn repository_string(&self) -> String {
        format!("github:{}/{}", self.owner, self.repo)
    }
}

impl fmt::Display for SourceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "github:{}/{}", self.owner, self.repo)?;
        if let Some(ref r) = self.git_ref {
            write!(f, "@{}", r)?;
        }
        if let Some(ref m) = self.module_path {
            write!(f, "#{}", m)?;
        }
        Ok(())
    }
}

/// Provenance information written to `source.toml` alongside an installed module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub repository: String,
    pub git_ref: Option<String>,
    pub commit: String,
    pub path: Option<String>,
    pub installed_at: u64,
}

/// Wrapper for TOML serialization with `[source]` table.
#[derive(Debug, Serialize, Deserialize)]
pub struct SourceFile {
    pub source: SourceInfo,
}
