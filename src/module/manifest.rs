// Module manifest (TOML) parsing and data types.

use anyhow::{bail, Result};
use serde::de;
use serde::{Deserialize, Deserializer};

use crate::core::safety;

/// How the contents of a target can be restored after deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestoreKind {
    /// Rebuilt automatically by the system, zero user action needed.
    #[default]
    Auto,
    /// Requires a manual step to restore (e.g. `npm install`, `pod install`).
    Manual,
}

impl<'de> Deserialize<'de> for RestoreKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "auto" => Ok(RestoreKind::Auto),
            "manual" => Ok(RestoreKind::Manual),
            other => Err(de::Error::custom(format!(
                "unknown restore kind '{}': expected one of auto, manual",
                other
            ))),
        }
    }
}

impl std::fmt::Display for RestoreKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreKind::Auto => write!(f, "auto"),
            RestoreKind::Manual => write!(f, "manual"),
        }
    }
}

/// Potential impact of deleting a target's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskLevel {
    /// No meaningful impact — safe to remove freely.
    #[default]
    Safe,
    /// Low impact — minor inconvenience at most.
    Low,
    /// May contain user data worth reviewing before deletion.
    Medium,
    /// Likely data loss without a backup.
    High,
}

impl<'de> Deserialize<'de> for RiskLevel {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "safe" => Ok(RiskLevel::Safe),
            "low" => Ok(RiskLevel::Low),
            "medium" => Ok(RiskLevel::Medium),
            "high" => Ok(RiskLevel::High),
            other => Err(de::Error::custom(format!(
                "unknown risk level '{}': expected one of safe, low, medium, high",
                other
            ))),
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Safe => write!(f, "safe"),
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
        }
    }
}

/// Represents a parsed module manifest (module.toml).
#[derive(Debug, Clone)]
pub struct Module {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub platforms: Vec<String>,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub targets: Vec<Target>,
}

/// Raw deserialization struct for TOML parsing.
#[derive(Debug, Deserialize)]
struct RawModule {
    id: String,
    name: String,
    version: String,
    description: String,
    author: String,
    platforms: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    icon: Option<String>,
    icon_color: Option<String>,
    targets: Vec<RawTarget>,
}

/// Intermediate target struct for TOML deserialization.
#[derive(Debug, Deserialize)]
struct RawTarget {
    #[serde(default, deserialize_with = "deserialize_optional_string_or_vec")]
    path: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_string_or_vec")]
    paths: Option<Vec<String>>,
    description: Option<String>,
    #[serde(default)]
    restore: RestoreKind,
    restore_steps: Option<String>,
    #[serde(default)]
    risk: RiskLevel,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    ignore: Vec<String>,
}

/// Deserialize a field that accepts either a single string or an array of strings.
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or list of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            seq: A,
        ) -> std::result::Result<Vec<String>, A::Error> {
            Vec::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

/// Deserialize an optional field that accepts either a single string or an array of strings.
fn deserialize_optional_string_or_vec<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_string_or_vec(deserializer).map(Some)
}

impl Module {
    /// Deserialize from a TOML string and validate.
    pub fn parse(toml_str: &str) -> Result<Module> {
        let raw: RawModule = toml::from_str(toml_str)?;
        validate_id(&raw.id)?;

        let icon = raw.icon.map(|s| resolve_icon_str(&s)).transpose()?;
        if let Some(ref icon) = icon {
            validate_icon(icon)?;
        }

        let mut targets = Vec::with_capacity(raw.targets.len());
        for raw_target in raw.targets {
            let paths = match (raw_target.path, raw_target.paths) {
                (Some(p), None) => p,
                (None, Some(ps)) => ps,
                (Some(_), Some(_)) => {
                    bail!("target must specify either 'path' or 'paths', not both");
                }
                (None, None) => {
                    bail!("target must specify either 'path' or 'paths'");
                }
            };
            if paths.is_empty() {
                bail!("target paths must not be empty");
            }

            for p in &paths {
                safety::validate_target_pattern(p)?;
            }

            for pattern in &raw_target.ignore {
                validate_ignore_pattern(pattern)?;
            }

            targets.push(Target {
                paths,
                description: raw_target.description,
                restore: raw_target.restore,
                restore_steps: raw_target.restore_steps,
                risk: raw_target.risk,
                ignore: raw_target.ignore,
            });
        }

        Ok(Module {
            id: raw.id,
            name: raw.name,
            version: raw.version,
            description: raw.description,
            author: raw.author,
            platforms: raw.platforms,
            tags: raw.tags,
            icon,
            icon_color: raw.icon_color,
            targets,
        })
    }
}

/// Resolve a `U+XXXX` or `U+XXXXX` escape to the actual character, or return
/// the string as-is if it is already a literal glyph.
fn resolve_icon_str(s: &str) -> Result<String> {
    let re_like =
        s.len() >= 6 && s.starts_with("U+") && s[2..].chars().all(|c| c.is_ascii_hexdigit());
    if re_like {
        let hex = &s[2..];
        let cp = u32::from_str_radix(hex, 16)
            .map_err(|_| anyhow::anyhow!("invalid Unicode escape: {s}"))?;
        let ch =
            char::from_u32(cp).ok_or_else(|| anyhow::anyhow!("invalid Unicode code point: {s}"))?;
        Ok(ch.to_string())
    } else {
        Ok(s.to_string())
    }
}

/// Validate that an icon is a single Nerd Font glyph (Private Use Area).
fn validate_icon(icon: &str) -> Result<()> {
    let mut chars = icon.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => {
            let cp = c as u32;
            if matches!(cp, 0xE000..=0xF8FF | 0xF0000..=0xF1AF0) {
                Ok(())
            } else {
                bail!(
                    "icon U+{:04X} is not a Nerd Font glyph; must be in PUA range U+E000-U+F8FF or U+F0000-U+F1AF0",
                    cp
                );
            }
        }
        (None, _) => bail!("icon must not be empty"),
        _ => bail!("icon must be a single character"),
    }
}

/// Validate that an ignore pattern is a relative glob (no `..` or absolute paths).
fn validate_ignore_pattern(pattern: &str) -> Result<()> {
    if pattern.is_empty() {
        bail!("ignore pattern must not be empty");
    }
    if pattern.starts_with('/') {
        bail!(
            "ignore pattern '{}' must be relative, not absolute",
            pattern
        );
    }
    if pattern.contains("..") {
        bail!(
            "ignore pattern '{}' must not contain directory traversal (..)",
            pattern
        );
    }
    // Verify it's a valid glob pattern
    if glob::Pattern::new(pattern).is_err() {
        bail!("ignore pattern '{}' is not a valid glob pattern", pattern);
    }
    Ok(())
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

/// A target that a module scans. Each entry in `paths` is either a fixed path
/// (supports `~` and glob `*`) or `**/dirname` for recursive local search.
#[derive(Debug, Clone)]
pub struct Target {
    pub paths: Vec<String>,
    pub description: Option<String>,
    pub restore: RestoreKind,
    pub restore_steps: Option<String>,
    pub risk: RiskLevel,
    /// Glob patterns for files/directories to preserve when cleaning this target.
    pub ignore: Vec<String>,
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
        assert_eq!(module.targets[0].paths, vec!["~/Library/Caches/test"]);
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
        assert_eq!(module.targets[0].paths, vec!["**/node_modules"]);
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

    // --- multi-path tests ---

    #[test]
    fn parse_paths_array() {
        let toml_str = r#"
        id = "multi-path"
        name = "multi-path"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        paths = ["~/Library/Caches/foo", "~/Library/Caches/bar"]
        description = "Multiple caches"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets.len(), 1);
        assert_eq!(
            module.targets[0].paths,
            vec!["~/Library/Caches/foo", "~/Library/Caches/bar"]
        );
    }

    #[test]
    fn parse_single_path_backward_compat() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert_eq!(module.targets[0].paths, vec!["~/Library/Caches/test"]);
    }

    #[test]
    fn parse_rejects_both_path_and_paths() {
        let toml_str = r#"
        id = "both"
        name = "both"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/foo"
        paths = ["~/bar"]
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("not both"));
    }

    #[test]
    fn parse_rejects_neither_path_nor_paths() {
        let toml_str = r#"
        id = "neither"
        name = "neither"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        description = "no path at all"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("either"));
    }

    #[test]
    fn parse_rejects_empty_paths_array() {
        let toml_str = r#"
        id = "empty-paths"
        name = "empty-paths"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        paths = []
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn parse_rejects_traversal_in_paths_array() {
        let toml_str = r#"
        id = "evil-paths"
        name = "evil-paths"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        paths = ["~/Library/Caches/ok", "~/Library/../../../etc/passwd"]
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn parse_multiple_valid_paths() {
        let toml_str = r#"
        id = "multi"
        name = "multi"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        paths = ["~/Library/Caches/a", "~/Library/Caches/b", "~/Library/Caches/c"]
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].paths.len(), 3);
        assert_eq!(module.targets[0].paths[0], "~/Library/Caches/a");
        assert_eq!(module.targets[0].paths[1], "~/Library/Caches/b");
        assert_eq!(module.targets[0].paths[2], "~/Library/Caches/c");
    }

    #[test]
    fn parse_path_accepts_list() {
        let toml_str = r#"
        id = "path-list"
        name = "path-list"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = ["~/Library/Caches/foo", "~/Library/Caches/bar"]
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(
            module.targets[0].paths,
            vec!["~/Library/Caches/foo", "~/Library/Caches/bar"]
        );
    }

    #[test]
    fn parse_paths_accepts_single_string() {
        let toml_str = r#"
        id = "paths-str"
        name = "paths-str"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        paths = "~/Library/Caches/foo"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].paths, vec!["~/Library/Caches/foo"]);
    }

    #[test]
    fn parse_tags() {
        let toml_str = r#"
        id = "tagged"
        name = "tagged"
        version = "1.0.0"
        description = "Tagged module"
        author = "tester"
        platforms = ["macos"]
        tags = ["cache", "build-artifacts"]
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.tags, vec!["cache", "build-artifacts"]);
    }

    #[test]
    fn parse_empty_tags() {
        let toml_str = r#"
        id = "empty-tags"
        name = "empty-tags"
        version = "1.0.0"
        description = "Empty tags"
        author = "tester"
        platforms = ["macos"]
        tags = []
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert!(module.tags.is_empty());
    }

    #[test]
    fn parse_missing_tags_defaults_to_empty() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert!(module.tags.is_empty());
    }

    // --- icon validation ---

    #[test]
    fn parse_nerd_font_icon() {
        let icon = "\u{e7a8}"; // Rust icon (Nerd Font)
        let toml_str = format!(
            r#"
        id = "icon-test"
        name = "icon-test"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "{icon}"
        targets = []
        "#
        );
        let module = Module::parse(&toml_str).unwrap();
        assert_eq!(module.icon.as_deref(), Some("\u{e7a8}"));
    }

    #[test]
    fn parse_icon_color() {
        let icon = "\u{e7a8}";
        let toml_str = format!(
            r##"
        id = "color-test"
        name = "color-test"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "{icon}"
        icon_color = "#CE422B"
        targets = []
        "##
        );
        let module = Module::parse(&toml_str).unwrap();
        assert_eq!(module.icon_color.as_deref(), Some("#CE422B"));
    }

    #[test]
    fn parse_missing_icon_defaults_to_none() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert!(module.icon.is_none());
        assert!(module.icon_color.is_none());
    }

    #[test]
    fn parse_rejects_emoji_icon() {
        let toml_str = r#"
        id = "emoji"
        name = "emoji"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "🐳"
        targets = []
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("Nerd Font"));
    }

    #[test]
    fn parse_rejects_multi_char_icon() {
        let toml_str = r#"
        id = "multi"
        name = "multi"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "ab"
        targets = []
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("single character"));
    }

    #[test]
    fn parse_rejects_empty_icon() {
        let toml_str = r#"
        id = "empty-icon"
        name = "empty-icon"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = ""
        targets = []
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn parse_icon_unicode_escape_4digit() {
        let toml_str = r#"
        id = "uplus"
        name = "uplus"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "U+E7A8"
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.icon.as_deref(), Some("\u{e7a8}"));
    }

    #[test]
    fn parse_icon_unicode_escape_5digit() {
        let toml_str = r#"
        id = "uplus5"
        name = "uplus5"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "U+F0000"
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.icon.as_deref(), Some("\u{F0000}"));
    }

    #[test]
    fn parse_icon_unicode_escape_lowercase() {
        let toml_str = r#"
        id = "uplus-lower"
        name = "uplus-lower"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "U+e7a8"
        targets = []
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.icon.as_deref(), Some("\u{e7a8}"));
    }

    #[test]
    fn parse_icon_unicode_escape_rejects_non_pua() {
        let toml_str = r#"
        id = "uplus-bad"
        name = "uplus-bad"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]
        icon = "U+0041"
        targets = []
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("Nerd Font"));
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

    // --- restore / risk tests ---

    #[test]
    fn parse_restore_and_risk() {
        let toml_str = r#"
        id = "risk-test"
        name = "risk-test"
        version = "1.0.0"
        description = "test"
        author = "tester"
        platforms = ["macos"]

        [[targets]]
        path = "~/Library/Caches/test"
        description = "Test cache"
        restore = "manual"
        restore_steps = "Run `foo install`"
        risk = "low"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].restore, RestoreKind::Manual);
        assert_eq!(
            module.targets[0].restore_steps.as_deref(),
            Some("Run `foo install`")
        );
        assert_eq!(module.targets[0].risk, RiskLevel::Low);
    }

    #[test]
    fn parse_all_risk_levels() {
        for (level_str, expected) in [
            ("safe", RiskLevel::Safe),
            ("low", RiskLevel::Low),
            ("medium", RiskLevel::Medium),
            ("high", RiskLevel::High),
        ] {
            let toml_str = format!(
                r#"
                id = "r"
                name = "r"
                version = "1.0.0"
                description = "t"
                author = "t"
                platforms = ["macos"]

                [[targets]]
                path = "~/tmp"
                risk = "{}"
                "#,
                level_str
            );
            let module = Module::parse(&toml_str).unwrap();
            assert_eq!(module.targets[0].risk, expected);
        }
    }

    #[test]
    fn parse_all_restore_kinds() {
        for (kind_str, expected) in [("auto", RestoreKind::Auto), ("manual", RestoreKind::Manual)] {
            let toml_str = format!(
                r#"
                id = "r"
                name = "r"
                version = "1.0.0"
                description = "t"
                author = "t"
                platforms = ["macos"]

                [[targets]]
                path = "~/tmp"
                restore = "{}"
                "#,
                kind_str
            );
            let module = Module::parse(&toml_str).unwrap();
            assert_eq!(module.targets[0].restore, expected);
        }
    }

    #[test]
    fn parse_defaults() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert_eq!(module.targets[0].risk, RiskLevel::Safe);
        assert_eq!(module.targets[0].restore, RestoreKind::Auto);
        assert!(module.targets[0].restore_steps.is_none());
    }

    #[test]
    fn parse_invalid_risk_level_rejected() {
        let toml_str = r#"
        id = "bad"
        name = "bad"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        risk = "unknown"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("unknown risk level"));
    }

    // --- ignore tests ---

    #[test]
    fn parse_ignore_single_string() {
        let toml_str = r#"
        id = "ign"
        name = "ign"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        ignore = "config.json"
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].ignore, vec!["config.json"]);
    }

    #[test]
    fn parse_ignore_list() {
        let toml_str = r#"
        id = "ign"
        name = "ign"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        ignore = ["config.json", "*.lock"]
        "#;
        let module = Module::parse(toml_str).unwrap();
        assert_eq!(module.targets[0].ignore, vec!["config.json", "*.lock"]);
    }

    #[test]
    fn parse_ignore_omitted_defaults_empty() {
        let module = Module::parse(valid_global_toml()).unwrap();
        assert!(module.targets[0].ignore.is_empty());
    }

    #[test]
    fn parse_ignore_rejects_absolute_path() {
        let toml_str = r#"
        id = "ign"
        name = "ign"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        ignore = "/etc/passwd"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("relative"));
    }

    #[test]
    fn parse_ignore_rejects_traversal() {
        let toml_str = r#"
        id = "ign"
        name = "ign"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        ignore = "../secret"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn parse_ignore_rejects_empty_string() {
        let toml_str = r#"
        id = "ign"
        name = "ign"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        ignore = ""
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn parse_invalid_restore_kind_rejected() {
        let toml_str = r#"
        id = "bad"
        name = "bad"
        version = "1.0.0"
        description = "t"
        author = "t"
        platforms = ["macos"]

        [[targets]]
        path = "~/tmp"
        restore = "unknown"
        "#;
        let err = Module::parse(toml_str).unwrap_err();
        assert!(err.to_string().contains("unknown restore kind"));
    }
}
