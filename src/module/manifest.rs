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

/// A target that a module scans. Either a global target (has `path`) or a local
/// target (has `name`) that is discovered by searching configured directories.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Target {
    pub path: Option<String>,
    pub name: Option<String>,
    pub indicator: Option<String>,
    pub description: Option<String>,
}
