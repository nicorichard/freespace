// Source identifier parsing and provenance tracking for installed modules.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

const DEFAULT_GIT_HOST: &str = "github.com";

/// Errors that can occur when parsing a source identifier.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("invalid source format: expected owner/repo, a git URL, or a local path")]
    InvalidFormat,
}

/// A parsed source identifier — either a git repo or a local directory path.
#[derive(Debug, Clone)]
pub enum SourceIdentifier {
    Git {
        host: String,
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
    /// Recognized formats:
    /// - `github:owner/repo[@ref][#module]` -> Git (github.com)
    /// - `https://host/owner/repo[.git][@ref][#module]` -> Git
    /// - `git@host:owner/repo[.git][@ref][#module]` -> Git
    /// - `owner/repo[@ref][#module]` (shorthand, defaults to github.com) -> Git
    /// - Anything else -> Local (filesystem path)
    pub fn parse(s: &str) -> Result<Self, SourceError> {
        // github:owner/repo — legacy/shorthand prefix
        if let Some(rest) = s.strip_prefix("github:") {
            return Self::parse_owner_repo(DEFAULT_GIT_HOST, rest);
        }

        // git:host/owner/repo — generic git prefix (used in source.toml for non-GitHub hosts)
        if let Some(rest) = s.strip_prefix("git:") {
            return Self::parse_host_owner_repo(rest);
        }

        // https://host/owner/repo[.git]
        if let Some(rest) = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
        {
            let rest = rest.trim_end_matches('/');
            let rest = rest.strip_suffix(".git").unwrap_or(rest);
            return Self::parse_host_owner_repo(rest);
        }

        // git@host:owner/repo[.git]
        if let Some(rest) = s.strip_prefix("git@") {
            // Format: git@host:owner/repo[.git]
            if let Some((host, path)) = rest.split_once(':') {
                let path = path.trim_end_matches('/');
                let path = path.strip_suffix(".git").unwrap_or(path);
                if !host.is_empty() {
                    return Self::parse_owner_repo(host, path);
                }
            }
            return Err(SourceError::InvalidFormat);
        }

        // owner/repo shorthand → defaults to github.com
        if looks_like_shorthand(s) {
            return Self::parse_owner_repo(DEFAULT_GIT_HOST, s);
        }

        // Treat as a local path
        let path = PathBuf::from(s);
        Ok(SourceIdentifier::Local { path })
    }

    /// Parse `host/owner/repo[@ref][#module]` extracting the host from the first segment.
    fn parse_host_owner_repo(rest: &str) -> Result<Self, SourceError> {
        // Split on '#' first for module path
        let (main_part, module_path) = match rest.split_once('#') {
            Some((main, module)) => {
                if module.is_empty() {
                    return Err(SourceError::InvalidFormat);
                }
                (main, Some(module.to_string()))
            }
            None => (rest, None),
        };

        // Split on '@' for optional git ref
        let (path_part, git_ref) = match main_part.split_once('@') {
            Some((p, r)) => {
                if r.is_empty() {
                    return Err(SourceError::InvalidFormat);
                }
                (p, Some(r.to_string()))
            }
            None => (main_part, None),
        };

        // Expect host/owner/repo
        let parts: Vec<&str> = path_part.splitn(3, '/').collect();
        if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(SourceError::InvalidFormat);
        }

        Ok(SourceIdentifier::Git {
            host: parts[0].to_string(),
            owner: parts[1].to_string(),
            repo: parts[2].to_string(),
            git_ref,
            module_path,
        })
    }

    /// Parse `owner/repo[@ref][#module]` with a known host.
    fn parse_owner_repo(host: &str, rest: &str) -> Result<Self, SourceError> {
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

        Ok(SourceIdentifier::Git {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref,
            module_path,
        })
    }

    /// Clone URLs in order of preference (HTTPS first, SSH fallback).
    pub fn clone_urls(&self) -> Vec<String> {
        match self {
            SourceIdentifier::Git {
                host, owner, repo, ..
            } => vec![
                format!("https://{}/{}/{}.git", host, owner, repo),
                format!("git@{}:{}/{}.git", host, owner, repo),
            ],
            SourceIdentifier::Local { .. } => vec![],
        }
    }

    /// The repository string for provenance tracking (stored in source.toml).
    ///
    /// For github.com, uses `github:owner/repo` for backward compatibility.
    /// For other hosts, uses `git:host/owner/repo`.
    pub fn repository_string(&self) -> String {
        match self {
            SourceIdentifier::Git {
                host, owner, repo, ..
            } => {
                if host == DEFAULT_GIT_HOST {
                    format!("github:{}/{}", owner, repo)
                } else {
                    format!("git:{}/{}/{}", host, owner, repo)
                }
            }
            SourceIdentifier::Local { path } => {
                format!("local:{}", path.display())
            }
        }
    }

    /// The directory basename to use when installing.
    pub fn default_dir_name(&self) -> String {
        match self {
            SourceIdentifier::Git { repo, .. } => repo.clone(),
            SourceIdentifier::Local { path } => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }

    /// The git ref, if any (Git only).
    pub fn git_ref(&self) -> Option<&String> {
        match self {
            SourceIdentifier::Git { git_ref, .. } => git_ref.as_ref(),
            SourceIdentifier::Local { .. } => None,
        }
    }

    /// The module path filter, if any (Git only).
    pub fn module_path(&self) -> Option<&String> {
        match self {
            SourceIdentifier::Git { module_path, .. } => module_path.as_ref(),
            SourceIdentifier::Local { .. } => None,
        }
    }
}

/// Check if a string looks like an `owner/repo` shorthand (defaults to github.com).
///
/// Must have exactly one `/`, and must not look like a filesystem path
/// (no leading `.`, `/`, `~`, and no `..` segments).
fn looks_like_shorthand(s: &str) -> bool {
    let slash_count = s.chars().filter(|&c| c == '/').count();
    if slash_count != 1 {
        return false;
    }

    if s.starts_with('/')
        || s.starts_with('.')
        || s.starts_with('~')
        || s.contains("://")
        || s.contains("..")
    {
        return false;
    }

    let (owner, rest) = s.split_once('/').unwrap();
    let repo = rest.split(&['@', '#'][..]).next().unwrap_or("");
    !owner.is_empty() && !repo.is_empty()
}

impl fmt::Display for SourceIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceIdentifier::Git {
                host,
                owner,
                repo,
                git_ref,
                module_path,
            } => {
                if host == DEFAULT_GIT_HOST {
                    write!(f, "github:{}/{}", owner, repo)?;
                } else {
                    write!(f, "git:{}/{}/{}", host, owner, repo)?;
                }
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

    fn assert_git(src: &SourceIdentifier, host: &str, owner: &str, repo: &str) {
        match src {
            SourceIdentifier::Git {
                host: h,
                owner: o,
                repo: r,
                ..
            } => {
                assert_eq!(h, host);
                assert_eq!(o, owner);
                assert_eq!(r, repo);
            }
            _ => panic!("expected Git variant"),
        }
    }

    // --- github: prefix ---

    #[test]
    fn parse_github_basic() {
        let src = SourceIdentifier::parse("github:user/repo").unwrap();
        assert_git(&src, "github.com", "user", "repo");
    }

    #[test]
    fn parse_github_with_ref() {
        let src = SourceIdentifier::parse("github:user/repo@v1.0.0").unwrap();
        assert_git(&src, "github.com", "user", "repo");
        assert_eq!(src.git_ref().map(|s| s.as_str()), Some("v1.0.0"));
    }

    #[test]
    fn parse_github_with_module() {
        let src = SourceIdentifier::parse("github:user/repo#my-module").unwrap();
        assert_git(&src, "github.com", "user", "repo");
        assert_eq!(src.module_path().map(|s| s.as_str()), Some("my-module"));
    }

    #[test]
    fn parse_github_with_ref_and_module() {
        let src = SourceIdentifier::parse("github:user/repo@main#docker").unwrap();
        assert_git(&src, "github.com", "user", "repo");
        assert_eq!(src.git_ref().map(|s| s.as_str()), Some("main"));
        assert_eq!(src.module_path().map(|s| s.as_str()), Some("docker"));
    }

    // --- HTTPS URLs ---

    #[test]
    fn parse_https_github() {
        let src =
            SourceIdentifier::parse("https://github.com/nicorichard/freespace-modules").unwrap();
        assert_git(&src, "github.com", "nicorichard", "freespace-modules");
    }

    #[test]
    fn parse_https_with_git_suffix() {
        let src = SourceIdentifier::parse("https://github.com/user/repo.git").unwrap();
        assert_git(&src, "github.com", "user", "repo");
    }

    #[test]
    fn parse_https_trailing_slash() {
        let src = SourceIdentifier::parse("https://github.com/user/repo/").unwrap();
        assert_git(&src, "github.com", "user", "repo");
    }

    #[test]
    fn parse_https_custom_host() {
        let src = SourceIdentifier::parse("https://gitlab.com/team/project").unwrap();
        assert_git(&src, "gitlab.com", "team", "project");
    }

    #[test]
    fn parse_https_self_hosted() {
        let src = SourceIdentifier::parse("https://git.internal.co/org/modules.git").unwrap();
        assert_git(&src, "git.internal.co", "org", "modules");
    }

    // --- SSH URLs ---

    #[test]
    fn parse_ssh_github() {
        let src = SourceIdentifier::parse("git@github.com:user/repo.git").unwrap();
        assert_git(&src, "github.com", "user", "repo");
    }

    #[test]
    fn parse_ssh_custom_host() {
        let src = SourceIdentifier::parse("git@gitlab.com:team/project.git").unwrap();
        assert_git(&src, "gitlab.com", "team", "project");
    }

    // --- Shorthand (defaults to github.com) ---

    #[test]
    fn parse_shorthand() {
        let src = SourceIdentifier::parse("nicorichard/freespace-modules").unwrap();
        assert_git(&src, "github.com", "nicorichard", "freespace-modules");
    }

    #[test]
    fn parse_shorthand_with_ref() {
        let src = SourceIdentifier::parse("user/repo@v2.0").unwrap();
        assert_git(&src, "github.com", "user", "repo");
        assert_eq!(src.git_ref().map(|s| s.as_str()), Some("v2.0"));
    }

    #[test]
    fn parse_shorthand_with_module() {
        let src = SourceIdentifier::parse("user/repo#docker").unwrap();
        assert_git(&src, "github.com", "user", "repo");
        assert_eq!(src.module_path().map(|s| s.as_str()), Some("docker"));
    }

    // --- Local paths ---

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
        assert!(matches!(src, SourceIdentifier::Local { .. }));
    }

    #[test]
    fn parse_tilde_path_not_git() {
        let src = SourceIdentifier::parse("~/modules/test").unwrap();
        assert!(matches!(src, SourceIdentifier::Local { .. }));
    }

    // --- Error cases ---

    #[test]
    fn parse_github_missing_repo() {
        assert!(SourceIdentifier::parse("github:user").is_err());
    }

    #[test]
    fn parse_github_empty_owner() {
        assert!(SourceIdentifier::parse("github:/repo").is_err());
    }

    #[test]
    fn parse_github_empty_repo() {
        assert!(SourceIdentifier::parse("github:user/").is_err());
    }

    #[test]
    fn parse_github_empty_ref() {
        assert!(SourceIdentifier::parse("github:user/repo@").is_err());
    }

    #[test]
    fn parse_github_empty_module() {
        assert!(SourceIdentifier::parse("github:user/repo#").is_err());
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
    fn clone_urls_custom_host() {
        let src = SourceIdentifier::parse("https://gitlab.com/team/project").unwrap();
        assert_eq!(
            src.clone_urls(),
            vec![
                "https://gitlab.com/team/project.git".to_string(),
                "git@gitlab.com:team/project.git".to_string(),
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
    fn default_dir_name_git() {
        let src = SourceIdentifier::parse("github:user/my-modules").unwrap();
        assert_eq!(src.default_dir_name(), "my-modules");
    }

    #[test]
    fn default_dir_name_local() {
        let src = SourceIdentifier::parse("/home/user/my-module").unwrap();
        assert_eq!(src.default_dir_name(), "my-module");
    }

    // --- Display ---

    #[test]
    fn display_github_full() {
        let src = SourceIdentifier::parse("github:user/repo@v1#mod").unwrap();
        assert_eq!(src.to_string(), "github:user/repo@v1#mod");
    }

    #[test]
    fn display_custom_host() {
        let src = SourceIdentifier::parse("https://gitlab.com/team/project").unwrap();
        assert_eq!(src.to_string(), "git:gitlab.com/team/project");
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
    fn repository_string_custom_host() {
        let src = SourceIdentifier::parse("https://gitlab.com/team/project").unwrap();
        assert_eq!(src.repository_string(), "git:gitlab.com/team/project");
    }

    #[test]
    fn repository_string_local() {
        let src = SourceIdentifier::parse("/tmp/test").unwrap();
        assert_eq!(src.repository_string(), "local:/tmp/test");
    }

    // --- Round-trip: source.toml repository strings parse back correctly ---

    #[test]
    fn roundtrip_github_repository_string() {
        let src = SourceIdentifier::parse("github:user/repo").unwrap();
        let repo_str = src.repository_string();
        let parsed = SourceIdentifier::parse(&repo_str).unwrap();
        assert_git(&parsed, "github.com", "user", "repo");
    }

    #[test]
    fn roundtrip_custom_host_repository_string() {
        let src = SourceIdentifier::parse("https://gitlab.com/team/project").unwrap();
        let repo_str = src.repository_string();
        let parsed = SourceIdentifier::parse(&repo_str).unwrap();
        assert_git(&parsed, "gitlab.com", "team", "project");
    }
}
