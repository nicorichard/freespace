use std::path::PathBuf;

/// Expand a leading `~` or `~/` to the user's home directory.
///
/// This is the single canonical tilde-expansion function for the entire codebase.
/// All call sites must use this (or re-export it) to ensure consistent behavior.
pub fn expand_tilde(path: &str) -> PathBuf {
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
}
