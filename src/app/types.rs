// Core type definitions used across the application.

use std::path::PathBuf;

use crate::module::manifest::Module;

/// Severity level for flash messages shown in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashLevel {
    Info,
    Warning,
    Error,
}

/// Which view is currently displayed.
#[derive(Clone, Copy)]
pub enum View {
    ModuleList,
    ModuleDetail(usize),
    CleanupConfirm,
    CleanupProgress,
    Help,
    Info(usize),
    FlatView,
    FileBrowser,
}

/// State for a single loaded module including its discovered items.
pub struct ModuleState {
    pub module: Module,
    pub items: Vec<Item>,
    pub total_size: Option<u64>,
    pub status: ModuleStatus,
    /// Filesystem path to the module's manifest (module.toml).
    pub manifest_path: Option<PathBuf>,
}

/// Loading/discovery status of a module.
pub enum ModuleStatus {
    Loading,
    Discovering,
    Ready,
    Error(String),
}

/// Overall scan status.
pub enum ScanStatus {
    Scanning,
    Complete,
}

/// A discovered filesystem item within a module.
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub item_type: ItemType,
    pub target_description: Option<String>,
    pub safety_level: crate::core::safety::SafetyLevel,
    /// Whether this item's path is also claimed by another module.
    pub is_shared: bool,
    /// How the contents can be restored after deletion.
    pub restore_kind: crate::module::manifest::RestoreKind,
    /// Human-readable recovery steps for this item.
    pub restore_steps: Option<String>,
    /// Potential impact of deleting this item's contents.
    pub risk_level: crate::module::manifest::RiskLevel,
}

/// The type of a discovered filesystem item.
pub enum ItemType {
    File,
    Directory,
}

/// Tracks the state of a background cleanup operation for rendering.
pub struct CleanupProgressState {
    /// Total number of items to process.
    pub total: usize,
    /// Number of items processed so far.
    pub done: usize,
    /// Path of the most recently processed item.
    pub current_path: Option<String>,
    /// Whether the operation is permanent delete (true) or trash (false).
    pub permanent: bool,
    /// Whether the user has requested to halt (pressed q/Ctrl+C).
    pub halted: bool,
}
