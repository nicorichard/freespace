// Module manifest (TOML) parsing and data types.

use serde::Deserialize;

/// Represents a parsed module manifest (module.toml).
#[derive(Debug, Clone, Deserialize)]
pub struct Module {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub platforms: Vec<String>,
    pub targets: Vec<Target>,
}

/// A target path pattern that a module scans.
#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub path: String,
    pub description: Option<String>,
}
