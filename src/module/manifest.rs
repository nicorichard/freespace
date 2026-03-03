// Module manifest (TOML) parsing and data types.

use anyhow::{bail, Result};
use serde::Deserialize;

use crate::core::safety;

/// Represents a parsed module manifest (module.toml).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Module {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub platforms: Vec<String>,
    pub targets: Vec<Target>,
}

impl Module {
    /// Deserialize from a TOML string and validate.
    pub fn parse(toml_str: &str) -> Result<Module> {
        let module: Module = toml::from_str(toml_str)?;
        validate_id(&module.id)?;
        for target in &module.targets {
            safety::validate_target_pattern(&target.path)?;
        }
        Ok(module)
    }
}

/// Validate that a module id is kebab-case: `^[a-z0-9]+(-[a-z0-9]+)*$`.
fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("module id must not be empty");
    }
    // Validate kebab-case: segments of [a-z0-9]+ separated by single hyphens,
    // no leading/trailing hyphens, no consecutive hyphens.
    let valid = id.split('-').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    if !valid {
        bail!(
            "module id '{}' is invalid: must be kebab-case (e.g. \"docker\", \"node-modules\")",
            id
        );
    }
    Ok(())
}

/// A target that a module scans. Uses `path` for fixed paths (supports `~` and
/// glob `*`) or `**/dirname` for recursive local search across search directories.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Target {
    pub path: String,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_global_toml() -> &'static str {
        r#"
        id = "test-module"
        name = "test-module"
        version = "1.0.0"
        description = "A test module"
        author = "tester"
        platforms = ["macos", "linux"]

        [[targets]]
        path = "~/Library/Caches/test"
        description = "Test cache"
        "#
    }

    #[test]
    fn parse_valid_global_target() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert_eq!(module.id, "test-module");
        assert_eq!(module.name, "test-module");
        assert_eq!(module.version, "1.0.0");
        assert_eq!(module.targets.len(), 1);
        assert_eq!(module.targets[0].path, "~/Library/Caches/test");
    }

    #[test]
    fn parse_valid_local_target() {
        let toml_str = r#"
        id = "node-modules"
        name = "node-modules"
        version = "1.0.0"
        description = "Node modules"
        author = "tester"
        platforms = ["macos", "linux"]

        [[targets]]
        path = "**/node_modules"
        description = "Node dependencies"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].path, "**/node_modules");
    }

    #[test]
    fn parse_multiple_targets() {
        let toml_str = r#"
        id = "multi"
        name = "multi"
        version = "1.0.0"
        description = "Multiple targets"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/Library/Caches/foo"
        description = "Foo"

        [[targets]]
        path = "**/bar"
        description = "Bar"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets.len(), 2);
    }

    #[test]
    fn parse_missing_required_fields() {
        let toml_str = r#"
        id = "incomplete"
        name = "incomplete"
        "#;
        assert!(Module::parse(toml_str).is_err());
    }

    #[test]
    fn parse_empty_targets() {
        let toml_str = r#"
        id = "empty"
        name = "empty"
        version = "1.0.0"
        description = "No targets"
        author = "tester"
        platforms = ["macos"]
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert!(module.targets.is_empty());
    }

    #[test]
    fn parse_optional_description_on_target() {
        let toml_str = r#"
        id = "nodesc"
        name = "nodesc"
        version = "1.0.0"
        description = "Module desc"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert!(module.targets[0].description.is_none());
    }

    // --- id validation ---

    #[test]
    fn valid_id_simple() {
        assert!(validate_id("docker").is_ok());
    }

    #[test]
    fn valid_id_kebab() {
        assert!(validate_id("node-modules").is_ok());
    }

    #[test]
    fn valid_id_with_digits() {
        assert!(validate_id("xcode-16").is_ok());
    }

    #[test]
    fn invalid_id_empty() {
        assert!(validate_id("").is_err());
    }

    #[test]
    fn invalid_id_spaces() {
        assert!(validate_id("Node Modules").is_err());
    }

    #[test]
    fn invalid_id_uppercase() {
        assert!(validate_id("UPPER").is_err());
    }

    #[test]
    fn invalid_id_trailing_hyphen() {
        assert!(validate_id("trailing-").is_err());
    }

    #[test]
    fn invalid_id_leading_hyphen() {
        assert!(validate_id("-leading").is_err());
    }

    #[test]
    fn parse_rejects_traversal_in_target() {
        let toml_str = r#"
        id = "evil"
        name = "evil"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/Library/../../../etc/passwd"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn parse_rejects_invalid_id() {
        let toml_str = r#"
        id = "Bad Id"
        name = "Bad Id"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        targets = []
        "#;
        assert!(Module::parse(toml_str).is_err());
    }
}
