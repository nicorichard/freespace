// Application state and event loop.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::module::manifest::Module;
use crate::tui::theme::Theme;

/// Central application state shared across all TUI views.
pub struct App {
    pub modules: Vec<ModuleState>,
    pub current_view: View,
    pub selected_index: usize,
    pub selected_items: HashSet<PathBuf>,
    pub scan_status: ScanStatus,
    pub config: AppConfig,
    pub theme: Theme,
}

/// Which view is currently displayed.
pub enum View {
    ModuleList,
    ModuleDetail(usize),
    CleanupConfirm,
    Help,
}

/// State for a single loaded module including its discovered items.
pub struct ModuleState {
    pub module: Module,
    pub items: Vec<Item>,
    pub total_size: Option<u64>,
    pub status: ModuleStatus,
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
    Idle,
    Scanning,
    Complete,
}

/// A discovered filesystem item within a module.
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub item_type: ItemType,
}

/// The type of a discovered filesystem item.
pub enum ItemType {
    File,
    Directory,
}
