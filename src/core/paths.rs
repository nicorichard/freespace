use std::path::{Path, PathBuf};

/// Expand a leading `~` or `~/` to the user's home directory.
///
/// This is the single canonical tilde-expansion function for the entire codebase.
/// All call sites must use this (or re-export it) to ensure consistent behavior.
pub fn expand_tilde(path: &str) -> PathBuf {
    match dirs::home_dir() {
        Some(home) => expand_tilde_in(path, &home),
        None => PathBuf::from(path),
    }
}

/// Expand a leading `~` against an explicit home directory.
///
/// Takes the home directory as an argument so callers working with a home that
/// is not the process's own — tests, most of all — get the same expansion.
pub fn expand_tilde_in(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

/// Render a path with the home directory written back as `~`.
///
/// The inverse of [`expand_tilde`], used when a discovered path is about to be
/// shown or written to config: `~/Projects` survives a move between machines
/// and a rename of the home directory, `/Users/someone/Projects` does not.
pub fn contract_tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_tilde() {
        let result = expand_tilde("~");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        }
    }

    #[test]
    fn tilde_slash_prefix() {
        let result = expand_tilde("~/Documents");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("Documents"));
        }
    }

    #[test]
    fn absolute_path_unchanged() {
        assert_eq!(expand_tilde("/usr/local"), PathBuf::from("/usr/local"));
    }

    #[test]
    fn relative_path_unchanged() {
        assert_eq!(expand_tilde("foo/bar"), PathBuf::from("foo/bar"));
    }

    #[test]
    fn contract_tilde_rewrites_home_prefix() {
        let home = PathBuf::from("/Users/someone");
        assert_eq!(
            contract_tilde(&home.join("Projects"), &home),
            "~/Projects".to_string()
        );
    }

    #[test]
    fn contract_tilde_of_home_itself() {
        let home = PathBuf::from("/Users/someone");
        assert_eq!(contract_tilde(&home, &home), "~".to_string());
    }

    #[test]
    fn contract_tilde_leaves_outside_paths_alone() {
        let home = PathBuf::from("/Users/someone");
        assert_eq!(
            contract_tilde(Path::new("/opt/work"), &home),
            "/opt/work".to_string()
        );
    }

    #[test]
    fn contract_tilde_round_trips_through_expand() {
        if let Some(home) = dirs::home_dir() {
            let path = home.join("Projects");
            assert_eq!(expand_tilde(&contract_tilde(&path, &home)), path);
        }
    }
}
