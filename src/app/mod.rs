// Application state and event loop.

mod drill;
mod filter;
mod types;

pub use drill::{DrillLevel, DrillState};
pub use filter::matches_filter;
pub use types::{
    CleanupProgressState, FlashLevel, Item, ItemType, ModuleState, ModuleStatus, ScanStatus, View,
};

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::core::cleaner::{self, CleanupMessage, CleanupOptions};
use crate::core::safety;
use crate::core::scanner::{self, ScanMessage};
use crate::module::manager;
use crate::module::manifest::Module;
use crate::tui::theme::Theme;
use crate::tui::views;
use crate::tui::Tui;

/// Tick rate for the event loop poll interval.
const TICK_RATE: Duration = Duration::from_millis(250);

/// Central application state shared across all TUI views.
pub struct App {
    pub modules: Vec<ModuleState>,
    pub current_view: View,
    pub selected_index: usize,
    pub selected_items: BTreeSet<PathBuf>,
    pub scan_status: ScanStatus,
    pub theme: Theme,
    /// View to return to when leaving an overlay (CleanupConfirm, Help).
    pub previous_view: View,
    /// Whether the application should exit.
    pub should_quit: bool,
    /// Whether the user is currently typing in the filter bar.
    pub filter_active: bool,
    /// Current filter text (empty = no filter).
    pub filter_query: String,
    /// Cursor position within the filter query.
    pub filter_cursor: usize,
    /// Receiver for scan messages from the background scanner.
    scan_rx: mpsc::UnboundedReceiver<ScanMessage>,
    /// Sender for scan messages (kept for drill-in size calculations).
    scan_tx: mpsc::UnboundedSender<ScanMessage>,
    /// Counter incremented each event loop tick, used for spinner animation.
    pub tick_count: usize,
    /// Whether the info overlay is showing a remove confirmation prompt.
    pub info_confirm_remove: bool,
    /// Deferred editor launch: set to a path to open in $EDITOR after key handling.
    pub pending_editor: Option<PathBuf>,
    /// Drill-in state for directory exploration.
    pub drill: DrillState,
    /// Saved selected_index for the module list so we can restore it on back navigation.
    pub module_list_index: usize,
    /// Saved selected_index for the flat view so we can restore it on back navigation.
    pub flat_view_index: usize,
    /// View to return to when the drill stack empties in FileBrowser.
    pub browser_origin: View,
    /// Module context for the browsed directory (needed for breadcrumb rendering).
    pub browser_module_idx: usize,
    /// Total disk capacity in bytes (root filesystem).
    pub disk_total: Option<u64>,
    /// Free disk space in bytes (root filesystem).
    pub disk_free: Option<u64>,
    /// Whether to simulate cleanup without deleting.
    pub dry_run: bool,
    /// User-configured protected paths (expanded to absolute).
    pub protected_paths: Vec<PathBuf>,
    /// Whether to write audit log entries.
    pub audit_log: bool,
    /// Whether the safety config enforces home-directory scope.
    pub enforce_scope: bool,
    /// Flash message displayed temporarily in the status bar.
    pub flash_message: Option<(String, FlashLevel)>,
    /// Remaining ticks before the flash message auto-clears.
    pub flash_ticks: usize,
    /// Paths blocked by safety rules during scanning (path, reason, module id, module name).
    blocked_paths: Vec<(PathBuf, String, String, String)>,
    /// Canonical paths already counted toward the deduped total.
    seen_paths: HashSet<PathBuf>,
    /// Accurate global total counting each unique path once across modules.
    pub deduped_total: u64,
    /// Receiver for cleanup progress messages from the background task.
    cleanup_rx: Option<mpsc::UnboundedReceiver<CleanupMessage>>,
    /// Cancel token for the in-progress cleanup task.
    pub(crate) cleanup_cancel: Option<Arc<AtomicBool>>,
    /// Progress state for the cleanup-in-progress view.
    pub cleanup_progress: Option<CleanupProgressState>,
    /// Items checked for cleanup in the confirmation view (subset of selected_items).
    pub confirm_checked: BTreeSet<PathBuf>,
    /// Persistent scroll offset for table views.
    pub view_offset: usize,
    /// Last left-click position and time, for double-click detection.
    last_click: Option<(Instant, u16, u16)>,
}

impl App {
    /// Create a new App with default state, load modules, and start scanning.
    ///
    /// When `directory_mode` is true (positional path arg was provided), only local
    /// `**/pattern` targets are scanned and CLI search dirs replace config dirs.
    pub fn new(
        cli_module_dirs: Vec<String>,
        cli_search_dirs: Vec<String>,
        dry_run: bool,
        directory_mode: bool,
    ) -> Self {
        let (modules, search_dirs, config) =
            Self::load_modules_and_config(cli_module_dirs, cli_search_dirs, directory_mode);

        let protected_paths = safety::expand_protected_paths(&config.protected_paths);
        let audit_log = config.audit_log;

        // Create channel for scan messages
        let (tx, rx) = mpsc::unbounded_channel();

        // Collect module manifests for the scanner
        let manifests: Vec<Module> = modules.iter().map(|ms| ms.module.clone()).collect();

        // Start background scan
        let scan_status = if manifests.is_empty() {
            ScanStatus::Complete
        } else {
            scanner::start_scan(manifests, tx.clone(), search_dirs);
            ScanStatus::Scanning
        };

        let (disk_total, disk_free) = disk_stats().unzip();

        Self {
            modules,
            current_view: View::ModuleList,
            selected_index: 0,
            selected_items: BTreeSet::new(),
            scan_status,
            theme: Theme::default(),
            previous_view: View::ModuleList,
            should_quit: false,
            filter_active: false,
            filter_query: String::new(),
            filter_cursor: 0,
            scan_rx: rx,
            scan_tx: tx,
            tick_count: 0,
            info_confirm_remove: false,
            pending_editor: None,
            drill: DrillState::new(),
            module_list_index: 0,
            flat_view_index: 0,
            browser_origin: View::ModuleList,
            browser_module_idx: 0,
            disk_total,
            disk_free,
            dry_run,
            protected_paths,
            audit_log,
            enforce_scope: config.enforce_scope,
            flash_message: None,
            flash_ticks: 0,
            blocked_paths: Vec::new(),
            seen_paths: HashSet::new(),
            deduped_total: 0,
            cleanup_rx: None,
            cleanup_cancel: None,
            cleanup_progress: None,
            confirm_checked: BTreeSet::new(),
            view_offset: 0,
            last_click: None,
        }
    }

    /// Return paths that were blocked by safety rules during scanning.
    pub fn blocked_paths(&self) -> &[(PathBuf, String, String, String)] {
        &self.blocked_paths
    }

    /// Determine the checkbox state for a given path based on the current selection.
    ///
    /// Uses `BTreeSet::range` for efficient prefix matching instead of
    /// iterating the entire selection set.
    pub fn check_state(&self, path: &Path) -> crate::tui::widgets::CheckState {
        use crate::tui::widgets::CheckState;

        // Exact match: this path is directly selected
        if self.selected_items.contains(path) {
            return CheckState::All;
        }

        // Ancestor selected: walk up path components
        let mut ancestor = path.to_path_buf();
        while ancestor.pop() {
            if self.selected_items.contains(&ancestor) {
                return CheckState::All;
            }
        }

        // Descendant selected: check if any selected path starts_with this path
        // BTreeSet::range gives us O(log n) seek to the first candidate
        let probe = path.to_path_buf();
        for selected in self.selected_items.range(probe..) {
            if selected.starts_with(path) {
                // Skip exact match (already handled above)
                if selected != path {
                    return CheckState::Partial;
                }
            } else {
                break;
            }
        }

        CheckState::None
    }

    /// Change the current view and reset scroll offset to the top.
    pub(crate) fn set_view(&mut self, view: View) {
        self.current_view = view;
        self.view_offset = 0;
    }

    /// Discover and load modules from all configured directories.
    /// Returns module states, expanded search_dirs paths, and the loaded config.
    fn load_modules_and_config(
        cli_module_dirs: Vec<String>,
        cli_search_dirs: Vec<String>,
        directory_mode: bool,
    ) -> (Vec<ModuleState>, Vec<PathBuf>, AppConfig) {
        // Load config file (warnings on failure, use defaults)
        let config = match AppConfig::load() {
            Ok(config) => config,
            Err(e) => {
                eprintln!("warning: {}", e);
                AppConfig::default()
            }
        };

        // Merge module dirs: config dirs first, then CLI dirs
        let mut extra_dirs = config.module_dirs.clone();
        extra_dirs.extend(cli_module_dirs);

        // If CLI provided explicit search dirs, use those instead of config
        let search_dir_strings = if cli_search_dirs.is_empty() {
            config.search_dirs.clone()
        } else {
            cli_search_dirs
        };

        // Expand tildes in search dirs
        let search_dirs: Vec<PathBuf> = search_dir_strings
            .iter()
            .map(|s| crate::core::paths::expand_tilde(s))
            .filter(|p| p.is_dir())
            .collect();

        let default_dir = crate::config::default_modules_dir();

        let (modules, warnings) = manager::load_all_modules(default_dir, &extra_dirs);

        // Log warnings to stderr (they won't be visible in the TUI but are
        // available if the user redirects stderr)
        for warning in &warnings {
            eprintln!("warning: {}", warning);
        }

        let module_states: Vec<ModuleState> = modules
            .into_iter()
            .filter_map(|(mut module, manifest_path)| {
                // In directory mode, only keep local (relative) targets
                if directory_mode {
                    module
                        .targets
                        .retain(|t| t.paths.iter().any(|p| p.starts_with("**/")));
                    if module.targets.is_empty() {
                        return None;
                    }
                }
                Some(ModuleState {
                    module,
                    items: Vec::new(),
                    total_size: None,
                    status: ModuleStatus::Loading,
                    manifest_path: Some(manifest_path),
                })
            })
            .collect();

        (module_states, search_dirs, config)
    }

    /// Run the main event loop: poll input -> update state -> render.
    pub fn run(&mut self, terminal: &mut Tui) -> anyhow::Result<()> {
        while !self.should_quit {
            // Render the current view
            terminal.draw(|frame| self.render(frame))?;

            // Poll for input events with tick rate timeout
            if event::poll(TICK_RATE)? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            self.handle_key(key.code, key.modifiers);
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse.kind, mouse.column, mouse.row);
                    }
                    Event::Resize(_, _) => {
                        // Immediate re-render on resize
                        continue;
                    }
                    _ => {}
                }
            }

            // Deferred editor launch (must happen outside key handler)
            if let Some(path) = self.pending_editor.take() {
                crate::tui::restore()?;
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                let _ = Command::new(&editor).arg(&path).status();
                *terminal = crate::tui::init()?;
            }

            // Process any pending scan messages (non-blocking)
            self.process_scan_messages();

            // Process any pending cleanup messages (non-blocking)
            self.process_cleanup_messages();

            // Increment tick counter for spinner animation
            self.tick_count = self.tick_count.wrapping_add(1);

            // Auto-clear flash messages
            if self.flash_ticks > 0 {
                self.flash_ticks -= 1;
                if self.flash_ticks == 0 {
                    self.flash_message = None;
                }
            }
        }

        Ok(())
    }

    /// Drain pending scan messages from the background scanner and update state.
    fn process_scan_messages(&mut self) {
        while let Ok(msg) = self.scan_rx.try_recv() {
            match msg {
                ScanMessage::ItemDiscovered {
                    module_index,
                    mut item,
                } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Discovering;
                        let (level, reason) = safety::classify_path(
                            &item.path,
                            &self.protected_paths,
                            self.enforce_scope,
                        );
                        if level == safety::SafetyLevel::Deny {
                            self.blocked_paths.push((
                                item.path.clone(),
                                reason.unwrap_or_else(|| "denied".to_string()),
                                ms.module.id.clone(),
                                ms.module.name.clone(),
                            ));
                        } else {
                            item.safety_level = level;
                            ms.items.push(item);
                        }
                    }
                }
                ScanMessage::ItemSized {
                    module_index,
                    item_index,
                    size,
                } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        let canonical = ms.items.get(item_index).map(|item| {
                            item.path
                                .canonicalize()
                                .unwrap_or_else(|_| item.path.clone())
                        });
                        let is_new = canonical
                            .as_ref()
                            .map(|c| self.seen_paths.insert(c.clone()))
                            .unwrap_or(true);

                        if let Some(item) = ms.items.get_mut(item_index) {
                            item.size = Some(size);
                            if !is_new {
                                item.is_shared = true;
                            }
                        }
                        if is_new {
                            self.deduped_total += size;
                        }
                        // Per-module total stays as-is (honest per-module size)
                        ms.total_size = Some(ms.total_size.unwrap_or(0) + size);
                    }
                }
                ScanMessage::ModuleComplete { module_index } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Ready;
                        // Ensure total_size is set even if no items were sized
                        if ms.total_size.is_none() {
                            ms.total_size = Some(0);
                        }
                    }
                }
                ScanMessage::ModuleError {
                    module_index,
                    error,
                } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Error(error);
                    }
                }
                ScanMessage::DrillItemSized {
                    drill_depth,
                    item_index,
                    size,
                } => {
                    self.drill.update_item_size(drill_depth, item_index, size);
                }
                ScanMessage::ScanComplete => {
                    self.scan_status = ScanStatus::Complete;
                }
            }
        }
    }

    /// List directory children as Items (instant, sizes are None).
    /// Applies safety classification to each entry and filters out denied paths.
    pub(crate) fn enumerate_directory(
        path: &Path,
        protected_paths: &[PathBuf],
        enforce_scope: bool,
    ) -> Vec<Item> {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let (level, _reason) =
                    safety::classify_path(&entry_path, protected_paths, enforce_scope);
                if level == safety::SafetyLevel::Deny {
                    continue;
                }
                let name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry_path.display().to_string());
                let item_type = if entry_path.is_dir() {
                    ItemType::Directory
                } else {
                    ItemType::File
                };
                items.push(Item {
                    name,
                    path: entry_path,
                    size: None,
                    item_type,
                    target_description: None,
                    safety_level: level,
                    is_shared: false,
                    restore_kind: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk_level: crate::module::manifest::RiskLevel::default(),
                });
            }
        }
        items
    }

    /// Open a path in the system file manager.
    pub(crate) fn open_in_file_manager(path: &Path) {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let cmd = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "explorer"
        } else {
            "xdg-open"
        };
        let _ = Command::new(cmd)
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    /// Spawn a background task to calculate sizes for drill-in items.
    pub(crate) fn spawn_drill_size_scan(&self, drill_depth: usize) {
        let tx = self.scan_tx.clone();
        let items = self.drill.scan_paths_at_depth(drill_depth);
        tokio::task::spawn_blocking(move || {
            for (item_index, path) in items.iter().enumerate() {
                let size = scanner::calculate_size(path);
                if tx
                    .send(ScanMessage::DrillItemSized {
                        drill_depth,
                        item_index,
                        size,
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
    }

    /// Dispatch key events based on the current view.
    pub fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Ctrl+C: during cleanup progress, halt or quit; otherwise always quit
        if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(self.current_view, View::CleanupProgress) {
                views::cleanup_progress::handle_key(self, KeyCode::Char('q'));
            } else {
                self.should_quit = true;
            }
            return;
        }

        // Normalize Emacs/terminal-style Ctrl keybindings to standard keys
        let key = crate::tui::widgets::normalize_emacs_key(key, modifiers);

        // If filter input is active, let navigation keys pass through
        // to the view handler (like fzf), handle everything else as filter input
        if self.filter_active {
            match key {
                KeyCode::Down | KeyCode::Up | KeyCode::Left | KeyCode::Right => {} // fall through to view handler
                _ => {
                    self.handle_key_filter(key);
                    return;
                }
            }
        }

        // Global: q quits from any view (but not during filter input or cleanup progress)
        if key == KeyCode::Char('q')
            && !self.filter_active
            && !matches!(self.current_view, View::CleanupProgress)
        {
            self.should_quit = true;
            return;
        }

        match &self.current_view {
            View::ModuleList => views::module_list::handle_key(self, key),
            View::ModuleDetail(_) => views::module_detail::handle_key(self, key),
            View::CleanupConfirm => views::cleanup_confirm::handle_key(self, key),
            View::CleanupProgress => views::cleanup_progress::handle_key(self, key),
            View::Help => views::help::handle_key(self, key),
            View::Info(idx) => {
                let idx = *idx;
                views::info::handle_key(self, key, idx);
            }
            View::FlatView => views::flat_view::handle_key(self, key),
            View::FileBrowser => views::file_browser::handle_key(self, key),
        }
    }

    /// Handle mouse events: scroll wheel and left-click.
    fn handle_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        match kind {
            MouseEventKind::ScrollUp => {
                self.view_offset = self.view_offset.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                self.view_offset = self.view_offset.saturating_add(1);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Double-click detection: if same row within 400ms, treat as Enter
                let now = Instant::now();
                let is_double = self
                    .last_click
                    .map(|(t, _c, r)| r == row && now.duration_since(t).as_millis() < 400)
                    .unwrap_or(false);
                self.last_click = Some((now, col, row));

                if is_double {
                    // Double-click → Enter on the already-selected row
                    self.handle_key(KeyCode::Enter, KeyModifiers::NONE);
                } else {
                    self.handle_click(col, row);
                }
            }
            _ => {}
        }
    }

    /// Handle a left-click by mapping screen coordinates to a list item.
    fn handle_click(&mut self, col: u16, row: u16) {
        // Get terminal size to recompute layout
        let (width, height) = match crossterm::terminal::size() {
            Ok(s) => s,
            Err(_) => return,
        };
        let area = ratatui::layout::Rect::new(0, 0, width, height);
        if area.width < Self::MIN_WIDTH {
            return;
        }

        match &self.current_view {
            View::ModuleList => views::module_list::handle_click(self, col, row, area),
            View::ModuleDetail(idx) => {
                let idx = *idx;
                views::module_detail::handle_click(self, col, row, area, idx);
            }
            View::FlatView => views::flat_view::handle_click(self, col, row, area),
            View::FileBrowser => views::file_browser::handle_click(self, col, row, area),
            View::CleanupConfirm => views::cleanup_confirm::handle_click(self, col, row, area),
            _ => {}
        }
    }

    /// Handle key input while the filter bar is active.
    fn handle_key_filter(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc => {
                // Cancel: clear query and exit filter mode
                self.filter_active = false;
                self.filter_query.clear();
                self.filter_cursor = 0;
                self.selected_index = 0;
            }
            KeyCode::Enter => {
                // Accept: keep query, exit filter mode
                self.filter_active = false;
                self.selected_index = 0;
            }
            KeyCode::Backspace => {
                self.filter_query.pop();
                self.filter_cursor = self.filter_query.len();
                self.selected_index = 0;
            }
            KeyCode::Char(c) => {
                self.filter_query.push(c);
                self.filter_cursor = self.filter_query.len();
                self.selected_index = 0;
            }
            _ => {}
        }
    }

    /// Show a flash message in the status bar for a number of ticks.
    pub(crate) fn set_flash(&mut self, text: impl Into<String>, level: FlashLevel) {
        self.flash_message = Some((text.into(), level));
        self.flash_ticks = 12; // ~3 seconds at 250ms tick rate
    }

    /// Clear filter state (used on view transitions).
    pub(crate) fn clear_filter(&mut self) {
        self.filter_active = false;
        self.filter_query.clear();
        self.filter_cursor = 0;
    }

    /// Return from FileBrowser to the originating view, restoring the saved index.
    /// `parent_idx` is the selected_index saved when the first drill level was pushed.
    pub(crate) fn return_from_file_browser(&mut self, parent_idx: usize) {
        match self.browser_origin {
            View::FlatView => {
                self.set_view(View::FlatView);
                self.selected_index = self.flat_view_index;
            }
            View::ModuleDetail(idx) => {
                self.set_view(View::ModuleDetail(idx));
                self.selected_index = parent_idx;
            }
            _ => {
                self.set_view(View::ModuleList);
                self.selected_index = self.module_list_index;
            }
        }
    }

    /// Spawn cleanup as a background blocking task, transitioning to CleanupProgress view.
    pub(crate) fn start_cleanup(&mut self, permanent: bool) {
        let paths: Vec<PathBuf> = self.confirm_checked.iter().cloned().collect();
        let total = paths.len();
        if total == 0 {
            return;
        }

        // Keep only unchecked items in the selection cart
        self.selected_items
            .retain(|p| !self.confirm_checked.contains(p));
        self.confirm_checked.clear();

        let opts = CleanupOptions {
            dry_run: self.dry_run,
            protected_paths: self.protected_paths.clone(),
            module_id: String::new(),
            audit_log: self.audit_log,
            enforce_scope: self.enforce_scope,
            allow_warned: true,
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        tokio::task::spawn_blocking(move || {
            let result = if permanent {
                cleaner::delete_items(&paths, &opts, &cancel_clone, &tx)
            } else {
                cleaner::trash_items(&paths, &opts, &cancel_clone, &tx)
            };
            let _ = tx.send(CleanupMessage::Complete(result));
        });

        self.cleanup_rx = Some(rx);
        self.cleanup_cancel = Some(cancel);
        self.cleanup_progress = Some(CleanupProgressState {
            total,
            done: 0,
            current_path: None,
            permanent,
            halted: false,
        });

        self.clear_filter();
        self.set_view(View::CleanupProgress);
    }

    /// Process cleanup messages from the background task.
    fn process_cleanup_messages(&mut self) {
        // Take rx out of self to avoid borrow conflicts when calling other &mut self methods.
        let mut rx = match self.cleanup_rx.take() {
            Some(rx) => rx,
            None => return,
        };

        let mut completed_result = None;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                CleanupMessage::Progress { done, total, path } => {
                    if let Some(progress) = &mut self.cleanup_progress {
                        progress.done = done;
                        progress.total = total;
                        progress.current_path = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .or_else(|| Some(path.display().to_string()));
                    }
                }
                CleanupMessage::Complete(result) => {
                    completed_result = Some(result);
                }
            }
        }

        if let Some(result) = completed_result {
            self.apply_cleanup_result(result);

            let halted = self.cleanup_progress.as_ref().is_some_and(|p| p.halted);
            if !halted {
                self.finish_cleanup();
            }
            // If halted, stay in CleanupProgress view for the confirmation.
            // Don't put rx back — cleanup is done.
        } else {
            // Cleanup still in progress, put rx back.
            self.cleanup_rx = Some(rx);
        }
    }

    /// Apply a cleanup result to module state and set flash messages.
    fn apply_cleanup_result(&mut self, result: cleaner::CleanupResult) {
        let succeeded: HashSet<PathBuf> = result.succeeded.into_iter().collect();
        let failed_count = result.failed.len();
        let total_count = succeeded.len() + failed_count;

        // Collect drill item sizes before clearing drill state
        let drill_item_sizes = self.drill.collect_item_sizes();

        for ms in &mut self.modules {
            ms.items.retain(|item| !succeeded.contains(&item.path));

            for item in &mut ms.items {
                for deleted_path in &succeeded {
                    if deleted_path.starts_with(&item.path) && deleted_path != &item.path {
                        if let (Some(parent_size), Some(child_size)) =
                            (item.size, drill_item_sizes.get(deleted_path))
                        {
                            item.size = Some(parent_size.saturating_sub(*child_size));
                        }
                    }
                }
            }

            let total: u64 = ms.items.iter().filter_map(|i| i.size).sum();
            ms.total_size = if ms.items.is_empty() {
                Some(0)
            } else {
                Some(total)
            };
        }

        // Flash message for failures
        if failed_count > 0 && failed_count == total_count {
            self.set_flash(
                format!(
                    "Blocked: {} item{} denied by safety rules",
                    failed_count,
                    if failed_count == 1 { "" } else { "s" }
                ),
                FlashLevel::Error,
            );
        } else if failed_count > 0 {
            let ok_count = succeeded.len();
            self.set_flash(
                format!(
                    "{}/{} cleaned; {} blocked by safety rules",
                    ok_count, total_count, failed_count
                ),
                FlashLevel::Warning,
            );
        }

        self.recalculate_dedup();
        self.drill.clear();
        self.selected_items.clear();
        self.refresh_disk_stats();
    }

    /// Finalize cleanup view: clear state and return to previous view.
    pub(crate) fn finish_cleanup(&mut self) {
        self.cleanup_rx = None;
        self.cleanup_cancel = None;
        self.cleanup_progress = None;
        self.confirm_checked.clear();
        self.set_view(self.previous_view);
        self.selected_index = 0;
    }

    /// Recalculate dedup state from all module items (e.g. after cleanup).
    fn recalculate_dedup(&mut self) {
        self.seen_paths.clear();
        self.deduped_total = 0;
        for ms in &mut self.modules {
            for item in &mut ms.items {
                if let Some(size) = item.size {
                    let canonical = item
                        .path
                        .canonicalize()
                        .unwrap_or_else(|_| item.path.clone());
                    if self.seen_paths.insert(canonical) {
                        self.deduped_total += size;
                        item.is_shared = false;
                    } else {
                        item.is_shared = true;
                    }
                }
            }
        }
    }

    /// Re-query disk stats and update fields.
    fn refresh_disk_stats(&mut self) {
        let (total, free) = disk_stats().unzip();
        self.disk_total = total;
        self.disk_free = free;
    }

    /// Minimum terminal width required for rendering views.
    const MIN_WIDTH: u16 = 80;

    /// Render the appropriate view based on current_view.
    fn render(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Show a message if the terminal is too narrow
        if area.width < Self::MIN_WIDTH {
            let msg = ratatui::widgets::Paragraph::new(format!(
                "Terminal too narrow ({}). Please resize to at least {} columns.",
                area.width,
                Self::MIN_WIDTH,
            ))
            .style(self.theme.style_warning());
            frame.render_widget(msg, area);
            return;
        }

        match self.current_view {
            View::ModuleList => views::module_list::render(self, frame),
            View::ModuleDetail(idx) => views::module_detail::render(self, frame, idx),
            View::CleanupConfirm => views::cleanup_confirm::render(self, frame),
            View::CleanupProgress => views::cleanup_progress::render(self, frame),
            View::Help => views::help::render(self, frame),
            View::Info(idx) => views::info::render(self, frame, idx),
            View::FlatView => views::flat_view::render(self, frame),
            View::FileBrowser => views::file_browser::render(self, frame),
        }
    }

    /// Build an App with controlled state for testing (no scanning, no config).
    #[cfg(test)]
    pub fn new_for_test(modules: Vec<ModuleState>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            modules,
            current_view: View::ModuleList,
            selected_index: 0,
            selected_items: BTreeSet::new(),
            scan_status: ScanStatus::Complete,
            theme: Theme::default(),
            previous_view: View::ModuleList,
            should_quit: false,
            filter_active: false,
            filter_query: String::new(),
            filter_cursor: 0,
            scan_rx: rx,
            scan_tx: tx,
            tick_count: 0,
            info_confirm_remove: false,
            pending_editor: None,
            drill: DrillState::new(),
            module_list_index: 0,
            flat_view_index: 0,
            browser_origin: View::ModuleList,
            browser_module_idx: 0,
            disk_total: None,
            disk_free: None,
            dry_run: false,
            protected_paths: Vec::new(),
            audit_log: false,
            enforce_scope: true,
            flash_message: None,
            flash_ticks: 0,
            blocked_paths: Vec::new(),
            seen_paths: HashSet::new(),
            deduped_total: 0,
            cleanup_rx: None,
            cleanup_cancel: None,
            cleanup_progress: None,
            confirm_checked: BTreeSet::new(),
            view_offset: 0,
            last_click: None,
        }
    }
}

/// Query total and free disk space for the root filesystem.
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn disk_stats() -> Option<(u64, u64)> {
    use std::ffi::CString;
    let path = CString::new("/").ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
    if ret != 0 {
        return None;
    }
    let total = stat.f_blocks as u64 * stat.f_frsize as u64;
    let free = stat.f_bavail as u64 * stat.f_frsize as u64;
    Some((total, free))
}

#[cfg(not(unix))]
fn disk_stats() -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::manifest::{Module, Target};

    /// Helper to create a test module with items.
    fn make_module(name: &str, items: Vec<(&str, u64)>) -> ModuleState {
        let module = Module {
            id: name.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            tags: vec![],
            targets: vec![Target {
                paths: vec!["~/test".to_string()],
                description: None,
                restore: crate::module::manifest::RestoreKind::default(),
                restore_steps: None,
                risk: crate::module::manifest::RiskLevel::default(),
            }],
        };
        let items = items
            .into_iter()
            .map(|(name, size)| Item {
                name: name.to_string(),
                path: PathBuf::from(format!("/tmp/test/{}", name)),
                size: Some(size),
                item_type: ItemType::Directory,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
                restore_kind: crate::module::manifest::RestoreKind::default(),
                restore_steps: None,
                risk_level: crate::module::manifest::RiskLevel::default(),
            })
            .collect();
        ModuleState {
            module,
            items,
            total_size: Some(0), // recalculated by sort
            status: ModuleStatus::Ready,
            manifest_path: None,
        }
    }

    fn make_test_app() -> App {
        let mut m1 = make_module(
            "docker",
            vec![("images", 5_000_000_000), ("volumes", 2_000_000_000)],
        );
        m1.total_size = Some(7_000_000_000);

        let mut m2 = make_module("npm-cache", vec![("_cacache", 1_000_000_000)]);
        m2.total_size = Some(1_000_000_000);

        App::new_for_test(vec![m1, m2])
    }

    // --- Navigation ---

    #[test]
    fn navigate_down_j() {
        let mut app = make_test_app();
        assert_eq!(app.selected_index, 0);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn navigate_down_arrow() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn navigate_up_k() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn navigate_up_wraps() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 1); // wraps to last
    }

    #[test]
    fn navigate_down_wraps() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected_index, 0); // wraps to first
    }

    #[test]
    fn enter_opens_detail() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        match app.current_view {
            View::ModuleDetail(_) => {}
            _ => panic!("expected ModuleDetail view"),
        }
    }

    // --- Selection ---

    #[test]
    fn space_toggles_module_selection() {
        let mut app = make_test_app();
        assert!(app.selected_items.is_empty());
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(!app.selected_items.is_empty());
        // Toggle again to deselect
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(app.selected_items.is_empty());
    }

    #[test]
    fn select_all_a() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // All items from both modules should be selected
        assert_eq!(app.selected_items.len(), 3);
    }

    #[test]
    fn deselect_all_n() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(app.selected_items.len(), 3);
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(app.selected_items.is_empty());
    }

    // --- View transitions ---

    #[test]
    fn q_quits() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits_during_filter() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        assert!(app.filter_active);
        app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn esc_from_detail_returns_to_list() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleDetail(_)));
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleList));
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::Help));
    }

    #[test]
    fn esc_from_help_returns() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::Help));
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleList));
    }

    // --- Filter ---

    #[test]
    fn slash_activates_filter() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        assert!(app.filter_active);
    }

    #[test]
    fn filter_typing_updates_query() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        assert_eq!(app.filter_query, "do");
    }

    #[test]
    fn filter_esc_clears() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.filter_active);
        assert!(app.filter_query.is_empty());
    }

    #[test]
    fn filter_enter_accepts() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.filter_active);
        assert_eq!(app.filter_query, "d"); // query kept
    }

    #[test]
    fn filter_backspace_removes_char() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.filter_query, "a");
    }

    // --- Cleanup flow ---

    #[test]
    fn c_with_selection_opens_confirm() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE); // select all
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::CleanupConfirm));
    }

    #[test]
    fn c_without_selection_does_nothing() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleList));
    }

    #[test]
    fn cleanup_n_cancels() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        let selected_count = app.selected_items.len();
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::CleanupConfirm));
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleList));
        // Cancel preserves selection
        assert_eq!(app.selected_items.len(), selected_count);
    }

    // --- Detail view selection ---

    #[test]
    fn detail_space_toggles_item() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE); // enter detail
        assert!(matches!(app.current_view, View::ModuleDetail(_)));
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(app.selected_items.len(), 1);
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(app.selected_items.is_empty());
    }

    #[test]
    fn detail_backspace_goes_back() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleDetail(_)));
        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        assert!(matches!(app.current_view, View::ModuleList));
    }

    // --- Ctrl+N/P emacs bindings ---

    #[test]
    fn ctrl_n_moves_down() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn ctrl_p_moves_up() {
        let mut app = make_test_app();
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert_eq!(app.selected_index, 0);
    }

    // --- Module list sort/filter logic ---

    #[test]
    fn module_list_sorted_excludes_zero_size() {
        let mut m = make_module("empty", vec![]);
        m.total_size = Some(0);
        let app = App::new_for_test(vec![m]);
        let sorted = crate::tui::views::module_list::sorted_module_indices(&app);
        assert!(sorted.is_empty()); // 0 B modules excluded from navigation
    }

    #[test]
    fn module_list_sorted_respects_filter() {
        let mut app = make_test_app();
        app.filter_query = "dock".to_string();
        let sorted = crate::tui::views::module_list::sorted_module_indices(&app);
        assert_eq!(sorted.len(), 1);
    }

    // --- Cleanup confirm ---

    #[test]
    fn collect_selected_items_sorted_by_size() {
        let mut app = make_test_app();
        // Select all items
        for ms in &app.modules {
            for item in &ms.items {
                app.selected_items.insert(item.path.clone());
            }
        }
        let items = crate::tui::views::cleanup_confirm::collect_selected_items(&app);
        assert_eq!(items.len(), 3);
        // Should be sorted by size descending
        let sizes: Vec<Option<u64>> = items.iter().map(|i| i.size).collect();
        for w in sizes.windows(2) {
            assert!(w[0] >= w[1]);
        }
    }
}
