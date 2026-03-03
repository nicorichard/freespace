// Module manifest (TOML) parsing and data types.

use serde::Deserialize;

/// Represents a parsed module manifest (module.toml).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Module {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub platforms: Vec<String>,
    pub targets: Vec<Target>,
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
        let module: Module = toml::from_str(valid_global_toml()).unwrap();
        assert_eq!(module.name, "test-module");
        assert_eq!(module.version, "1.0.0");
        assert_eq!(module.targets.len(), 1);
        assert_eq!(module.targets[0].path, "~/Library/Caches/test");
    }

    #[test]
    fn parse_valid_local_target() {
        let toml_str = r#"
        name = "node-modules"
        version = "1.0.0"
        description = "Node modules"
        author = "tester"
        platforms = ["macos", "linux"]

        [[targets]]
        path = "**/node_modules"
        description = "Node dependencies"
        "#;
        let module: Module = toml::from_str(toml_str).unwrap();
        assert_eq!(module.targets[0].path, "**/node_modules");
    }

    #[test]
    fn parse_multiple_targets() {
        let toml_str = r#"
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
        let module: Module = toml::from_str(toml_str).unwrap();
        assert_eq!(module.targets.len(), 2);
    }

    #[test]
    fn parse_missing_required_fields() {
        let toml_str = r#"
        name = "incomplete"
        "#;
        let result: Result<Module, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_targets() {
        let toml_str = r#"
        name = "empty"
        version = "1.0.0"
        description = "No targets"
        author = "tester"
        platforms = ["macos"]
        targets = []
        "#;
        let module: Module = toml::from_str(toml_str).unwrap();
        assert!(module.targets.is_empty());
    }

    #[test]
    fn parse_optional_description_on_target() {
        let toml_str = r#"
        name = "nodesc"
        version = "1.0.0"
        description = "Module desc"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        "#;
        let module: Module = toml::from_str(toml_str).unwrap();
        assert!(module.targets[0].description.is_none());
    }
}
