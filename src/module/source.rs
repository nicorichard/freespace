// Source identifier parsing and provenance tracking for installed modules.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Errors that can occur when parsing a source identifier.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("invalid github format: expected github:owner/repo[@ref][#module]")]
    InvalidFormat,
}

/// A parsed source identifier — either a GitHub repo or a local directory path.
#[derive(Debug, Clone)]
pub enum SourceIdentifier {
    GitHub {
        owner: String,
        repo: String,
        git_ref: Option<String>,
        module_path: Option<String>,
    },
    Local {
        path: PathBuf,
    },
}

impl SourceIdentifier {
    /// Parse a source string.
    ///
    /// - `github:user/repo@v1.0.0#module-name` -> GitHub variant
    /// - Anything else -> Local variant (treated as a filesystem path)
    pub fn parse(s: &str) -> Result<Self, SourceError> {
        if let Some(rest) = s.strip_prefix("github:") {
            return Self::parse_github(rest);
        }

        // Treat as a local path
        let path = PathBuf::from(s);
        Ok(SourceIdentifier::Local { path })
    }

    fn parse_github(rest: &str) -> Result<Self, SourceError> {
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

        Ok(SourceIdentifier::GitHub {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref,
            module_path,
        })
    }

    /// Clone URLs for GitHub sources, in order of preference (HTTPS first, SSH fallback).
    pub fn clone_urls(&self) -> Vec<String> {
        match self {
            SourceIdentifier::GitHub { owner, repo, .. } => vec![
                format!("https://github.com/{}/{}.git", owner, repo),
                format!("git@github.com:{}/{}.git", owner, repo),
            ],
            SourceIdentifier::Local { .. } => vec![],
        }
    }

    /// The repository string for provenance tracking.
    pub fn repository_string(&self) -> String {
        match self {
            SourceIdentifier::GitHub { owner, repo, .. } => {
                format!("github:{}/{}", owner, repo)
            }
            SourceIdentifier::Local { path } => {
                format!("local:{}", path.display())
            }
        }
    }

    /// The directory basename to use when installing.
    pub fn default_dir_name(&self) -> String {
        match self {
            SourceIdentifier::GitHub { repo, .. } => repo.clone(),
            SourceIdentifier::Local { path } => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }

    /// The git ref, if any (GitHub only).
    pub fn git_ref(&self) -> Option<&String> {
        match self {
            SourceIdentifier::GitHub { git_ref, .. } => git_ref.as_ref(),
            SourceIdentifier::Local { .. } => None,
        }
    }

    /// The module path filter, if any (GitHub only).
    pub fn module_path(&self) -> Option<&String> {
        match self {
            SourceIdentifier::GitHub { module_path, .. } => module_path.as_ref(),
            SourceIdentifier::Local { .. } => None,
        }
    }
}

impl fmt::Display for SourceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceIdentifier::GitHub {
                owner,
                repo,
                git_ref,
                module_path,
            } => {
                write!(f, "github:{}/{}", owner, repo)?;
                if let Some(ref r) = git_ref {
                    write!(f, "@{}", r)?;
                }
                if let Some(ref m) = module_path {
                    write!(f, "#{}", m)?;
                }
                Ok(())
            }
            SourceIdentifier::Local { path } => {
                write!(f, "{}", path.display())
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- SourceIdentifier::parse() ---

    #[test]
    fn parse_github_basic() {
        let src = SourceIdentifier::parse("github:user/repo").unwrap();
        match src {
            SourceIdentifier::GitHub {
                owner,
                repo,
                git_ref,
                module_path,
            } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert!(git_ref.is_none());
                assert!(module_path.is_none());
            }
            _ => panic!("expected GitHub variant"),
        }
    }

    #[test]
    fn parse_github_with_ref() {
        let src = SourceIdentifier::parse("github:user/repo@v1.0.0").unwrap();
        match src {
            SourceIdentifier::GitHub { git_ref, .. } => {
                assert_eq!(git_ref.as_deref(), Some("v1.0.0"));
            }
            _ => panic!("expected GitHub variant"),
        }
    }

    #[test]
    fn parse_github_with_module() {
        let src = SourceIdentifier::parse("github:user/repo#my-module").unwrap();
        match src {
            SourceIdentifier::GitHub { module_path, .. } => {
                assert_eq!(module_path.as_deref(), Some("my-module"));
            }
            _ => panic!("expected GitHub variant"),
        }
    }

    #[test]
    fn parse_github_with_ref_and_module() {
        let src = SourceIdentifier::parse("github:user/repo@main#docker").unwrap();
        match src {
            SourceIdentifier::GitHub {
                owner,
                repo,
                git_ref,
                module_path,
            } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(git_ref.as_deref(), Some("main"));
                assert_eq!(module_path.as_deref(), Some("docker"));
            }
            _ => panic!("expected GitHub variant"),
        }
    }

    #[test]
    fn parse_local_absolute_path() {
        let src = SourceIdentifier::parse("/tmp/my-module").unwrap();
        match src {
            SourceIdentifier::Local { path } => {
                assert_eq!(path, PathBuf::from("/tmp/my-module"));
            }
            _ => panic!("expected Local variant"),
        }
    }

    #[test]
    fn parse_local_relative_path() {
        let src = SourceIdentifier::parse("./modules/test").unwrap();
        match src {
            SourceIdentifier::Local { path } => {
                assert_eq!(path, PathBuf::from("./modules/test"));
            }
            _ => panic!("expected Local variant"),
        }
    }

    #[test]
    fn parse_github_missing_repo() {
        let result = SourceIdentifier::parse("github:user");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_empty_owner() {
        let result = SourceIdentifier::parse("github:/repo");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_empty_repo() {
        let result = SourceIdentifier::parse("github:user/");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_empty_ref() {
        let result = SourceIdentifier::parse("github:user/repo@");
        assert!(result.is_err());
    }

    #[test]
    fn parse_github_empty_module() {
        let result = SourceIdentifier::parse("github:user/repo#");
        assert!(result.is_err());
    }

    // --- clone_urls ---

    #[test]
    fn clone_urls_github() {
        let src = SourceIdentifier::parse("github:user/repo").unwrap();
        assert_eq!(
            src.clone_urls(),
            vec![
                "https://github.com/user/repo.git".to_string(),
                "git@github.com:user/repo.git".to_string(),
            ]
        );
    }

    #[test]
    fn clone_urls_local_is_empty() {
        let src = SourceIdentifier::parse("/tmp/foo").unwrap();
        assert!(src.clone_urls().is_empty());
    }

    // --- default_dir_name ---

    #[test]
    fn default_dir_name_github() {
        let src = SourceIdentifier::parse("github:user/my-modules").unwrap();
        assert_eq!(src.default_dir_name(), "my-modules");
    }

    #[test]
    fn default_dir_name_local() {
        let src = SourceIdentifier::parse("/home/user/my-module").unwrap();
        assert_eq!(src.default_dir_name(), "my-module");
    }

    // --- Display roundtrip ---

    #[test]
    fn display_github_full() {
        let src = SourceIdentifier::parse("github:user/repo@v1#mod").unwrap();
        assert_eq!(src.to_string(), "github:user/repo@v1#mod");
    }

    #[test]
    fn display_local() {
        let src = SourceIdentifier::parse("/tmp/test").unwrap();
        assert_eq!(src.to_string(), "/tmp/test");
    }

    // --- repository_string ---

    #[test]
    fn repository_string_github() {
        let src = SourceIdentifier::parse("github:user/repo@v1#mod").unwrap();
        assert_eq!(src.repository_string(), "github:user/repo");
    }

    #[test]
    fn repository_string_local() {
        let src = SourceIdentifier::parse("/tmp/test").unwrap();
        assert_eq!(src.repository_string(), "local:/tmp/test");
    }
}
