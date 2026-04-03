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
    ModuleInstall,
}

/// State for a single loaded module including its discovered items.
pub struct ModuleState {
    pub module: Module,
    pub items: Vec<Item>,
    pub total_size: Option<u64>,
    pub status: ModuleStatus,
    /// Filesystem path to the module's manifest (module.toml).
    pub manifest_path: Option<PathBuf>,
    /// Result of the background update check (None = not checked yet).
    pub update_status: Option<ModuleUpdateStatus>,
}

/// Result of checking a module for available updates.
pub enum ModuleUpdateStatus {
    /// Checking in progress.
    Checking,
    /// Module is up to date.
    UpToDate,
    /// A newer commit is available on the remote.
    UpdateAvailable { new_commit: String },
    /// A newer semver tag exists on the remote.
    NewerTagAvailable {
        current_tag: String,
        latest_tag: String,
    },
    /// Not applicable (local module, no source.toml, etc.)
    Skipped,
    /// Check failed.
    Failed(String),
}

/// State for the "update siblings?" confirmation modal in the info panel.
pub struct SiblingUpdatePrompt {
    /// The module the user pressed `u` on.
    pub current_idx: usize,
    /// Sibling module indices from the same repo that also have updates.
    pub sibling_indices: Vec<usize>,
    /// Currently highlighted choice (0 = all, 1 = this only, 2 = cancel).
    pub selected: usize,
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
    /// Glob patterns for files/directories to preserve when cleaning this item.
    pub ignore_patterns: Vec<String>,
}

/// The type of a discovered filesystem item.
pub enum ItemType {
    File,
    Directory,
}

/// Phase of the module install picker flow.
pub enum InstallPhase {
    /// Cloning the repository in the background.
    Cloning,
    /// Showing the picker for the user to select modules.
    Picking,
    /// Installing/removing selected modules in the background.
    Installing,
    /// Done — results are ready.
    Done,
}

/// A candidate module discovered in a source repo.
pub struct InstallCandidate {
    /// Directory name within the repo (used as install dir name).
    pub dir_name: String,
    /// Parsed module manifest.
    pub module: Module,
    /// Whether this module is currently checked (will be installed/kept).
    pub checked: bool,
    /// Whether this module was already installed before opening the picker.
    pub was_installed: bool,
}

/// State for the in-TUI module install picker view.
pub struct ModuleInstallState {
    /// The source string the user provided (e.g. "github:user/repo").
    pub source_str: String,
    /// Discovered candidate modules from the source.
    pub candidates: Vec<InstallCandidate>,
    /// Cursor position in the candidate list.
    pub cursor: usize,
    /// Current phase of the install flow.
    pub phase: InstallPhase,
    /// Path to the cloned/local source directory (for cleanup).
    pub source_dir: Option<PathBuf>,
    /// Commit SHA from the cloned repo (None for local sources).
    pub commit_sha: Option<String>,
    /// Result messages after installation completes.
    pub results: Vec<String>,
    /// Path to the modules install directory.
    pub modules_dir: PathBuf,
    /// Symlink local sources instead of copying.
    pub link: bool,
}

/// Messages sent from background install tasks to the event loop.
pub enum InstallMessage {
    /// Clone completed, modules discovered.
    CloneComplete {
        source_dir: PathBuf,
        commit_sha: Option<String>,
        candidates: Vec<(String, Module)>,
        already_installed: Vec<bool>,
    },
    /// Clone or detection failed.
    CloneFailed(String),
    /// Installation of selected modules completed.
    InstallComplete(Vec<String>),
    /// Installation failed.
    InstallFailed(String),
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
