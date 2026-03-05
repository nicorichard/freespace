// Application state and event loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::core::cleaner::{self, CleanupOptions};
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
    pub selected_items: HashSet<PathBuf>,
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
}

impl App {
    /// Create a new App with default state, load modules, and start scanning.
    pub fn new(cli_module_dirs: Vec<String>, cli_search_dirs: Vec<String>, dry_run: bool) -> Self {
        let (modules, search_dirs, config) =
            Self::load_modules_and_config(cli_module_dirs, cli_search_dirs);

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
            selected_items: HashSet::new(),
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
        }
    }

    /// Return paths that were blocked by safety rules during scanning.
    pub fn blocked_paths(&self) -> &[(PathBuf, String, String, String)] {
        &self.blocked_paths
    }

    /// Discover and load modules from all configured directories.
    /// Returns module states, expanded search_dirs paths, and the loaded config.
    fn load_modules_and_config(
        cli_module_dirs: Vec<String>,
        cli_search_dirs: Vec<String>,
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

        // Merge search dirs: config dirs first, then CLI dirs
        let mut search_dir_strings = config.search_dirs.clone();
        search_dir_strings.extend(cli_search_dirs);

        // Expand tildes in search dirs
        let search_dirs: Vec<PathBuf> = search_dir_strings
            .iter()
            .map(|s| {
                if let Some(rest) = s.strip_prefix("~/") {
                    if let Some(home) = dirs::home_dir() {
                        return home.join(rest);
                    }
                } else if s == "~" {
                    if let Some(home) = dirs::home_dir() {
                        return home;
                    }
                }
                PathBuf::from(s)
            })
            .filter(|p| p.is_dir())
            .collect();

        let default_dir = crate::config::default_modules_dir();

        let (modules, warnings) = manager::load_all_modules(default_dir, &extra_dirs);

        // Log warnings to stderr (they won't be visible in the TUI but are
        // available if the user redirects stderr)
        for warning in &warnings {
            eprintln!("warning: {}", warning);
        }

        let module_states = modules
            .into_iter()
            .map(|(module, manifest_path)| ModuleState {
                module,
                items: Vec::new(),
                total_size: None,
                status: ModuleStatus::Loading,
                manifest_path: Some(manifest_path),
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

    /// Return the items for the current detail view level.
    /// If drilled in, returns the drill level's items; otherwise the module's items.
    pub fn current_detail_items(&self, module_idx: usize) -> &[Item] {
        self.drill.items_or(&self.modules[module_idx].items)
    }

    /// List directory children as Items (instant, sizes are None).
    /// Applies safety classification to each entry and filters out denied paths.
    fn enumerate_directory(
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
                });
            }
        }
        items
    }

    /// Open a path in the system file manager (Finder on macOS).
    fn open_in_file_manager(path: &Path) {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = Command::new("open")
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    /// Spawn a background task to calculate sizes for drill-in items.
    fn spawn_drill_size_scan(&self, drill_depth: usize) {
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
        // Ctrl+C always quits, even during filter input
        if key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
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

        // Global: q quits from any view (but not during filter input)
        if key == KeyCode::Char('q') && !self.filter_active {
            self.should_quit = true;
            return;
        }

        match &self.current_view {
            View::ModuleList => self.handle_key_module_list(key),
            View::ModuleDetail(_) => self.handle_key_module_detail(key),
            View::CleanupConfirm => self.handle_key_cleanup_confirm(key),
            View::Help => self.handle_key_help(key),
            View::Info(idx) => {
                let idx = *idx;
                self.handle_key_info(key, idx);
            }
            View::FlatView => self.handle_key_flat_view(key),
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
    fn set_flash(&mut self, text: impl Into<String>, level: FlashLevel) {
        self.flash_message = Some((text.into(), level));
        self.flash_ticks = 12; // ~3 seconds at 250ms tick rate
    }

    /// Clear filter state (used on view transitions).
    fn clear_filter(&mut self) {
        self.filter_active = false;
        self.filter_query.clear();
        self.filter_cursor = 0;
    }

    // Stub key handlers for each view — will be implemented in later stories.

    fn handle_key_module_list(&mut self, key: KeyCode) {
        let sorted = views::module_list::sorted_module_indices(self);
        let count = sorted.len();

        match key {
            // Navigate down
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.selected_index = (self.selected_index + 1) % count;
                }
            }
            // Navigate up
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    self.selected_index = if self.selected_index == 0 {
                        count - 1
                    } else {
                        self.selected_index - 1
                    };
                }
            }
            // Enter detail view for selected module
            KeyCode::Enter => {
                if let Some(&module_idx) = sorted.get(self.selected_index) {
                    self.module_list_index = self.selected_index;
                    self.clear_filter();
                    self.current_view = View::ModuleDetail(module_idx);
                    self.selected_index = 0;
                }
            }
            // Toggle selection for all items in the focused module
            KeyCode::Char(' ') => {
                if let Some(&module_idx) = sorted.get(self.selected_index) {
                    let items = &self.modules[module_idx].items;
                    let all_selected = !items.is_empty()
                        && items
                            .iter()
                            .all(|item| self.selected_items.contains(&item.path));
                    if all_selected {
                        // Deselect all
                        for item in &self.modules[module_idx].items {
                            self.selected_items.remove(&item.path);
                        }
                    } else {
                        // Select all
                        for item in &self.modules[module_idx].items {
                            self.selected_items.insert(item.path.clone());
                        }
                    }
                }
            }
            // Select all items across all visible (filtered) modules
            KeyCode::Char('a') => {
                for &module_idx in &sorted {
                    let paths: Vec<PathBuf> = self.modules[module_idx]
                        .items
                        .iter()
                        .map(|item| item.path.clone())
                        .collect();
                    for path in paths {
                        self.selected_items.insert(path);
                    }
                }
            }
            // Deselect all items across all visible (filtered) modules
            KeyCode::Char('n') => {
                for &module_idx in &sorted {
                    let paths: Vec<PathBuf> = self.modules[module_idx]
                        .items
                        .iter()
                        .map(|item| item.path.clone())
                        .collect();
                    for path in paths {
                        self.selected_items.remove(&path);
                    }
                }
            }
            // Open help overlay
            KeyCode::Char('?') => {
                self.previous_view = self.current_view;
                self.current_view = View::Help;
            }
            // Transition to cleanup confirmation if items are selected
            KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.previous_view = self.current_view;
                    self.current_view = View::CleanupConfirm;
                    self.selected_index = 0;
                }
            }
            // Enter filter mode
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter_query.clear();
                self.filter_cursor = 0;
                self.selected_index = 0;
            }
            // Open info overlay for the selected module
            KeyCode::Char('i') => {
                if let Some(&module_idx) = sorted.get(self.selected_index) {
                    self.previous_view = self.current_view;
                    self.current_view = View::Info(module_idx);
                }
            }
            // Switch to flat view
            KeyCode::Tab => {
                self.module_list_index = self.selected_index;
                self.clear_filter();
                self.current_view = View::FlatView;
                self.selected_index = 0;
            }
            // Esc: clear filter
            KeyCode::Esc => {
                if !self.filter_query.is_empty() {
                    self.clear_filter();
                    self.selected_index = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_key_module_detail(&mut self, key: KeyCode) {
        let module_idx = match &self.current_view {
            View::ModuleDetail(idx) => *idx,
            _ => return,
        };

        if module_idx >= self.modules.len() {
            return;
        }

        let (display_order, group_boundaries) =
            views::module_detail::display_order_item_indices(self, module_idx);
        let count = display_order.len();

        match key {
            // Navigate down
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.selected_index = (self.selected_index + 1) % count;
                }
            }
            // Navigate up
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    self.selected_index = if self.selected_index == 0 {
                        count - 1
                    } else {
                        self.selected_index - 1
                    };
                }
            }
            // Jump to next target group
            KeyCode::Char('l') | KeyCode::Right => {
                if !group_boundaries.is_empty() {
                    // Find the next group boundary after current position
                    if let Some(&next) = group_boundaries.iter().find(|&&b| b > self.selected_index)
                    {
                        self.selected_index = next;
                    }
                }
            }
            // Jump to previous target group
            KeyCode::Char('h') | KeyCode::Left => {
                if !group_boundaries.is_empty() {
                    // Find the group boundary at or before current position,
                    // then jump to the one before that (or stay if at first group)
                    let current_group = group_boundaries
                        .iter()
                        .rposition(|&b| b <= self.selected_index);
                    if let Some(gi) = current_group {
                        if self.selected_index > group_boundaries[gi] {
                            // Not at start of current group — jump to its start
                            self.selected_index = group_boundaries[gi];
                        } else if gi > 0 {
                            // At start of current group — jump to previous group
                            self.selected_index = group_boundaries[gi - 1];
                        }
                    }
                }
            }
            // Toggle selection on highlighted item
            KeyCode::Char(' ') => {
                if let Some(&item_idx) = display_order.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    let path = items[item_idx].path.clone();
                    let meta = (items[item_idx].size, items[item_idx].safety_level);
                    if !self.selected_items.remove(&path) {
                        // Selecting parent: prune any already-selected children
                        self.selected_items.retain(|p| !p.starts_with(&path));
                        self.selected_items.insert(path.clone());
                        self.drill.cache_selection(path, meta);
                    } else {
                        self.drill.uncache_selection(&path);
                    }
                }
            }
            // Select all visible items
            KeyCode::Char('a') => {
                let snapshot: Vec<_> = self
                    .current_detail_items(module_idx)
                    .iter()
                    .map(|item| (item.path.clone(), item.size, item.safety_level))
                    .collect();
                for (path, size, safety) in snapshot {
                    self.selected_items.retain(|p| !p.starts_with(&path));
                    self.selected_items.insert(path.clone());
                    self.drill.cache_selection(path, (size, safety));
                }
            }
            // Deselect all visible items
            KeyCode::Char('n') => {
                let paths: Vec<PathBuf> = self
                    .current_detail_items(module_idx)
                    .iter()
                    .map(|item| item.path.clone())
                    .collect();
                for path in paths {
                    self.selected_items.remove(&path);
                    self.drill.uncache_selection(&path);
                }
            }
            // Enter: drill into directory, or cleanup if at top level
            KeyCode::Enter => {
                if let Some(&item_idx) = display_order.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    if matches!(items[item_idx].item_type, ItemType::Directory) {
                        let path = items[item_idx].path.clone();
                        let children = Self::enumerate_directory(
                            &path,
                            &self.protected_paths,
                            self.enforce_scope,
                        );
                        let parent_selected_index = self.selected_index;
                        self.drill.push(path, children, parent_selected_index);
                        self.clear_filter();
                        self.selected_index = 0;
                        let depth = self.drill.depth() - 1;
                        self.spawn_drill_size_scan(depth);
                    }
                }
            }
            // c: cleanup
            KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.previous_view = self.current_view;
                    self.current_view = View::CleanupConfirm;
                    self.selected_index = 0;
                }
            }
            // Open in file manager
            KeyCode::Char('o') => {
                if let Some(&item_idx) = display_order.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    Self::open_in_file_manager(&items[item_idx].path);
                }
            }
            // Open info overlay for this module
            KeyCode::Char('i') => {
                self.previous_view = self.current_view;
                self.current_view = View::Info(module_idx);
            }
            // Enter filter mode
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter_query.clear();
                self.filter_cursor = 0;
                self.selected_index = 0;
            }
            // Esc: clear filter → pop drill stack → go to ModuleList
            KeyCode::Esc => {
                if !self.filter_query.is_empty() {
                    self.clear_filter();
                    self.selected_index = 0;
                } else if let Some(parent_idx) = self.drill.pop() {
                    self.selected_index = parent_idx;
                    self.clear_filter();
                } else {
                    self.clear_filter();
                    self.current_view = View::ModuleList;
                    self.selected_index = self.module_list_index;
                }
            }
            // Backspace: pop drill stack if drilled in, else go back to module list
            KeyCode::Backspace => {
                if let Some(parent_idx) = self.drill.pop() {
                    self.selected_index = parent_idx;
                    self.clear_filter();
                } else {
                    self.clear_filter();
                    self.current_view = View::ModuleList;
                    self.selected_index = self.module_list_index;
                }
            }
            // Open help overlay
            KeyCode::Char('?') => {
                self.previous_view = self.current_view;
                self.current_view = View::Help;
            }
            _ => {}
        }
    }

    fn handle_key_flat_view(&mut self, key: KeyCode) {
        let sorted = views::flat_view::sorted_flat_items(self);
        let count = sorted.len();

        match key {
            // Navigate
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.selected_index = (self.selected_index + 1) % count;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    self.selected_index = if self.selected_index == 0 {
                        count - 1
                    } else {
                        self.selected_index - 1
                    };
                }
            }
            // Toggle selection
            KeyCode::Char(' ') => {
                if let Some(&(module_idx, item_idx)) = sorted.get(self.selected_index) {
                    let path = self.modules[module_idx].items[item_idx].path.clone();
                    if !self.selected_items.remove(&path) {
                        self.selected_items.retain(|p| !p.starts_with(&path));
                        self.selected_items.insert(path);
                    }
                }
            }
            // Select all visible items
            KeyCode::Char('a') => {
                for &(module_idx, item_idx) in &sorted {
                    let path = self.modules[module_idx].items[item_idx].path.clone();
                    self.selected_items.retain(|p| !p.starts_with(&path));
                    self.selected_items.insert(path);
                }
            }
            // Deselect all visible items
            KeyCode::Char('n') => {
                for &(module_idx, item_idx) in &sorted {
                    let path = self.modules[module_idx].items[item_idx].path.clone();
                    self.selected_items.remove(&path);
                }
            }
            // Enter: drill into directory
            KeyCode::Enter => {
                if let Some(&(module_idx, item_idx)) = sorted.get(self.selected_index) {
                    let item = &self.modules[module_idx].items[item_idx];
                    if matches!(item.item_type, ItemType::Directory) {
                        let path = item.path.clone();
                        let children = Self::enumerate_directory(
                            &path,
                            &self.protected_paths,
                            self.enforce_scope,
                        );
                        self.drill.push(path, children, 0);
                        self.current_view = View::ModuleDetail(module_idx);
                        self.clear_filter();
                        self.selected_index = 0;
                        let depth = self.drill.depth() - 1;
                        self.spawn_drill_size_scan(depth);
                    }
                }
            }
            // Cleanup
            KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.previous_view = self.current_view;
                    self.current_view = View::CleanupConfirm;
                    self.selected_index = 0;
                }
            }
            // Open in file manager
            KeyCode::Char('o') => {
                if let Some(&(module_idx, item_idx)) = sorted.get(self.selected_index) {
                    Self::open_in_file_manager(&self.modules[module_idx].items[item_idx].path);
                }
            }
            // Filter
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter_query.clear();
                self.filter_cursor = 0;
                self.selected_index = 0;
            }
            // Help
            KeyCode::Char('?') => {
                self.previous_view = self.current_view;
                self.current_view = View::Help;
            }
            // Tab: switch back to module list
            KeyCode::Tab => {
                self.clear_filter();
                self.current_view = View::ModuleList;
                self.selected_index = self.module_list_index;
            }
            // Esc: clear filter or go back to module list
            KeyCode::Esc => {
                if !self.filter_query.is_empty() {
                    self.clear_filter();
                    self.selected_index = 0;
                } else {
                    self.clear_filter();
                    self.current_view = View::ModuleList;
                    self.selected_index = self.module_list_index;
                }
            }
            _ => {}
        }
    }

    fn handle_key_cleanup_confirm(&mut self, key: KeyCode) {
        let count = views::cleanup_confirm::filtered_confirm_item_count(self);

        match key {
            // Navigate down
            KeyCode::Char('j') | KeyCode::Down => {
                if count > 0 {
                    self.selected_index = (self.selected_index + 1) % count;
                }
            }
            // Navigate up
            KeyCode::Char('k') | KeyCode::Up => {
                if count > 0 {
                    self.selected_index = if self.selected_index == 0 {
                        count - 1
                    } else {
                        self.selected_index - 1
                    };
                }
            }
            // Move to trash (reversible)
            KeyCode::Char('t') => {
                self.perform_cleanup(false);
                self.clear_filter();
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            // Permanently delete
            KeyCode::Char('d') => {
                self.perform_cleanup(true);
                self.clear_filter();
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            // Cancel and return to previous view
            KeyCode::Char('n') => {
                self.clear_filter();
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            // Esc: clear filter first, then close dialog
            KeyCode::Esc => {
                if !self.filter_query.is_empty() {
                    self.clear_filter();
                    self.selected_index = 0;
                } else {
                    self.current_view = self.previous_view;
                    self.selected_index = 0;
                }
            }
            // Enter filter mode
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter_query.clear();
                self.filter_cursor = 0;
                self.selected_index = 0;
            }
            _ => {}
        }
    }

    /// Perform live cleanup: delete selected items and update module state.
    /// When `permanent` is true, items are permanently deleted; otherwise they are moved to trash.
    fn perform_cleanup(&mut self, permanent: bool) {
        let paths: Vec<std::path::PathBuf> = self.selected_items.iter().cloned().collect();

        let opts = CleanupOptions {
            dry_run: self.dry_run,
            protected_paths: self.protected_paths.clone(),
            module_id: String::new(),
            audit_log: self.audit_log,
            enforce_scope: self.enforce_scope,
            allow_warned: true, // user confirmed via cleanup dialog
        };

        let result = if permanent {
            cleaner::delete_items(&paths, &opts)
        } else {
            cleaner::trash_items(&paths, &opts)
        };

        // Remove successfully deleted items from module state
        let succeeded: HashSet<PathBuf> = result.succeeded.into_iter().collect();

        // Collect drill item sizes before clearing drill state
        let drill_item_sizes = self.drill.collect_item_sizes();

        for ms in &mut self.modules {
            ms.items.retain(|item| !succeeded.contains(&item.path));

            // Update parent module items for deleted drill children
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

            // Recalculate total size from remaining items
            let total: u64 = ms.items.iter().filter_map(|i| i.size).sum();
            ms.total_size = if ms.items.is_empty() {
                Some(0)
            } else {
                Some(total)
            };
        }

        // Show flash message for failures
        let failed_count = result.failed.len();
        let total_count = paths.len();
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

        // Recalculate deduped_total and seen_paths from scratch
        self.recalculate_dedup();

        // Clear drill state and selected items
        self.drill.clear();
        self.selected_items.clear();

        // Refresh disk stats after cleanup
        self.refresh_disk_stats();
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

    fn handle_key_help(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('?') | KeyCode::Esc => {
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            _ => {}
        }
    }

    fn handle_key_info(&mut self, key: KeyCode, module_idx: usize) {
        if self.info_confirm_remove {
            match key {
                KeyCode::Char('y') => {
                    // Remove the module directory and state
                    if let Some(manifest_path) = &self.modules[module_idx].manifest_path {
                        if let Some(module_dir) = manifest_path.parent() {
                            let _ = std::fs::remove_dir_all(module_dir);
                        }
                    }
                    self.modules.remove(module_idx);
                    self.info_confirm_remove = false;
                    self.current_view = View::ModuleList;
                    self.selected_index = 0;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.info_confirm_remove = false;
                }
                _ => {}
            }
            return;
        }

        match key {
            KeyCode::Esc | KeyCode::Char('i') => {
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            KeyCode::Char('e') => {
                if let Some(manifest_path) = &self.modules[module_idx].manifest_path {
                    self.pending_editor = Some(manifest_path.clone());
                }
            }
            KeyCode::Char('o') => {
                if let Some(manifest_path) = &self.modules[module_idx].manifest_path {
                    if let Some(module_dir) = manifest_path.parent() {
                        Self::open_in_file_manager(module_dir);
                    }
                }
            }
            KeyCode::Char('r') => {
                self.info_confirm_remove = true;
            }
            _ => {}
        }
    }

    /// Minimum terminal width required for rendering views.
    const MIN_WIDTH: u16 = 80;

    /// Render the appropriate view based on current_view.
    fn render(&self, frame: &mut ratatui::Frame) {
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

        match &self.current_view {
            View::ModuleList => self.render_module_list(frame),
            View::ModuleDetail(idx) => self.render_module_detail(frame, *idx),
            View::CleanupConfirm => self.render_cleanup_confirm(frame),
            View::Help => self.render_help(frame),
            View::Info(idx) => self.render_info(frame, *idx),
            View::FlatView => self.render_flat_view(frame),
        }
    }

    // Placeholder render functions — will be replaced by full view implementations.

    fn render_module_list(&self, frame: &mut ratatui::Frame) {
        views::module_list::render(self, frame);
    }

    fn render_module_detail(&self, frame: &mut ratatui::Frame, module_idx: usize) {
        views::module_detail::render(self, frame, module_idx);
    }

    fn render_cleanup_confirm(&self, frame: &mut ratatui::Frame) {
        views::cleanup_confirm::render(self, frame);
    }

    fn render_help(&self, frame: &mut ratatui::Frame) {
        views::help::render(self, frame);
    }

    fn render_info(&self, frame: &mut ratatui::Frame, module_idx: usize) {
        views::info::render(self, frame, module_idx);
    }

    fn render_flat_view(&self, frame: &mut ratatui::Frame) {
        views::flat_view::render(self, frame);
    }

    /// Build an App with controlled state for testing (no scanning, no config).
    #[cfg(test)]
    pub fn new_for_test(modules: Vec<ModuleState>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            modules,
            current_view: View::ModuleList,
            selected_index: 0,
            selected_items: HashSet::new(),
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
    Help,
    Info(usize),
    FlatView,
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
#[allow(dead_code)]
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
    pub target_description: Option<String>,
    pub safety_level: crate::core::safety::SafetyLevel,
    /// Whether this item's path is also claimed by another module.
    pub is_shared: bool,
}

/// The type of a discovered filesystem item.
pub enum ItemType {
    File,
    Directory,
}

/// A level in the drill-in directory exploration stack.
pub struct DrillLevel {
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub parent_selected_index: usize,
}

/// Encapsulates drill-in state (stack + selection cache) and enforces invariants.
#[derive(Default)]
pub struct DrillState {
    stack: Vec<DrillLevel>,
    selection_cache: HashMap<PathBuf, (Option<u64>, safety::SafetyLevel)>,
}

impl DrillState {
    /// Create empty drill state.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            selection_cache: HashMap::new(),
        }
    }

    /// Whether the user is currently drilled into a directory.
    pub fn is_active(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Number of drill levels deep.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Return current drill items if active, otherwise the fallback slice.
    pub fn items_or<'a>(&'a self, fallback: &'a [Item]) -> &'a [Item] {
        if let Some(level) = self.stack.last() {
            &level.items
        } else {
            fallback
        }
    }

    /// Push a new drill level (enter a directory).
    pub fn push(&mut self, path: PathBuf, items: Vec<Item>, parent_selected_index: usize) {
        self.stack.push(DrillLevel {
            path,
            items,
            parent_selected_index,
        });
    }

    /// Pop the current drill level, returning the parent's selected index.
    pub fn pop(&mut self) -> Option<usize> {
        self.stack.pop().map(|level| level.parent_selected_index)
    }

    /// Update a drill item's size (from a `DrillItemSized` message) and sync the selection cache.
    pub fn update_item_size(&mut self, depth: usize, item_index: usize, size: u64) {
        if let Some(level) = self.stack.get_mut(depth) {
            if let Some(item) = level.items.get_mut(item_index) {
                item.size = Some(size);
                if let Some(cached) = self.selection_cache.get_mut(&item.path) {
                    cached.0 = Some(size);
                }
            }
        }
    }

    /// Cache metadata for a selected drill item so size/safety survives stack pops.
    pub fn cache_selection(&mut self, path: PathBuf, meta: (Option<u64>, safety::SafetyLevel)) {
        self.selection_cache.insert(path, meta);
    }

    /// Remove cached metadata for a deselected drill item.
    pub fn uncache_selection(&mut self, path: &Path) {
        self.selection_cache.remove(path);
    }

    /// Look up size and safety for a path: checks stack first, then cache.
    pub fn lookup_meta(&self, path: &Path) -> Option<(Option<u64>, safety::SafetyLevel)> {
        for level in &self.stack {
            if let Some(item) = level.items.iter().find(|i| i.path == path) {
                return Some((item.size, item.safety_level));
            }
        }
        self.selection_cache.get(path).copied()
    }

    /// Collect all drill item sizes into a map (for parent size adjustment during cleanup).
    pub fn collect_item_sizes(&self) -> HashMap<PathBuf, u64> {
        self.stack
            .iter()
            .flat_map(|level| level.items.iter())
            .filter_map(|item| item.size.map(|s| (item.path.clone(), s)))
            .collect()
    }

    /// Return breadcrumb directory names from the drill stack.
    pub fn breadcrumb_parts(&self) -> Vec<String> {
        self.stack
            .iter()
            .map(|level| {
                level
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| level.path.display().to_string())
            })
            .collect()
    }

    /// Return paths at the given drill depth for background size scanning.
    pub fn scan_paths_at_depth(&self, depth: usize) -> Vec<PathBuf> {
        self.stack
            .get(depth)
            .map(|level| level.items.iter().map(|i| i.path.clone()).collect())
            .unwrap_or_default()
    }

    /// Reset all drill state.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.selection_cache.clear();
    }
}

/// Case-insensitive substring match for filtering lists.
pub fn matches_filter(haystack: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    haystack.to_lowercase().contains(&query.to_lowercase())
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
            targets: vec![Target {
                path: "~/test".to_string(),
                description: None,
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

    // --- matches_filter ---

    #[test]
    fn matches_filter_empty_query() {
        assert!(matches_filter("anything", ""));
    }

    #[test]
    fn matches_filter_case_insensitive() {
        assert!(matches_filter("Docker", "docker"));
        assert!(matches_filter("docker", "DOCK"));
    }

    #[test]
    fn matches_filter_no_match() {
        assert!(!matches_filter("docker", "npm"));
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
        let sizes: Vec<Option<u64>> = items.iter().map(|i| i.2).collect();
        for w in sizes.windows(2) {
            assert!(w[0] >= w[1]);
        }
    }

    // --- DrillState ---

    #[test]
    fn drill_state_push_pop_restores_index() {
        let mut ds = DrillState::new();
        assert!(!ds.is_active());
        ds.push(PathBuf::from("/a"), vec![], 5);
        assert!(ds.is_active());
        assert_eq!(ds.depth(), 1);
        let idx = ds.pop();
        assert_eq!(idx, Some(5));
        assert!(!ds.is_active());
    }

    #[test]
    fn drill_state_items_or_fallback() {
        let ds = DrillState::new();
        let fallback = vec![Item {
            name: "x".into(),
            path: PathBuf::from("/x"),
            size: Some(1),
            item_type: ItemType::File,
            target_description: None,
            safety_level: crate::core::safety::SafetyLevel::Safe,
            is_shared: false,
        }];
        assert_eq!(ds.items_or(&fallback).len(), 1);
    }

    #[test]
    fn drill_state_items_or_drill() {
        let mut ds = DrillState::new();
        let drill_items = vec![
            Item {
                name: "a".into(),
                path: PathBuf::from("/a"),
                size: None,
                item_type: ItemType::File,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
            },
            Item {
                name: "b".into(),
                path: PathBuf::from("/b"),
                size: None,
                item_type: ItemType::File,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
            },
        ];
        ds.push(PathBuf::from("/dir"), drill_items, 0);
        let fallback: Vec<Item> = vec![];
        assert_eq!(ds.items_or(&fallback).len(), 2);
    }

    #[test]
    fn drill_state_update_item_size_syncs_cache() {
        let mut ds = DrillState::new();
        let items = vec![Item {
            name: "f".into(),
            path: PathBuf::from("/f"),
            size: None,
            item_type: ItemType::File,
            target_description: None,
            safety_level: crate::core::safety::SafetyLevel::Safe,
            is_shared: false,
        }];
        ds.push(PathBuf::from("/dir"), items, 0);
        ds.cache_selection(
            PathBuf::from("/f"),
            (None, crate::core::safety::SafetyLevel::Safe),
        );
        ds.update_item_size(0, 0, 42);
        let meta = ds.lookup_meta(Path::new("/f")).unwrap();
        assert_eq!(meta.0, Some(42));
    }

    #[test]
    fn drill_state_cache_uncache() {
        let mut ds = DrillState::new();
        ds.cache_selection(
            PathBuf::from("/x"),
            (Some(10), crate::core::safety::SafetyLevel::Warn),
        );
        assert!(ds.lookup_meta(Path::new("/x")).is_some());
        ds.uncache_selection(Path::new("/x"));
        assert!(ds.lookup_meta(Path::new("/x")).is_none());
    }

    #[test]
    fn drill_state_clear_resets() {
        let mut ds = DrillState::new();
        ds.push(PathBuf::from("/a"), vec![], 0);
        ds.cache_selection(
            PathBuf::from("/x"),
            (Some(1), crate::core::safety::SafetyLevel::Safe),
        );
        ds.clear();
        assert!(!ds.is_active());
        assert!(ds.lookup_meta(Path::new("/x")).is_none());
    }

    #[test]
    fn drill_state_lookup_meta_stack_first() {
        let mut ds = DrillState::new();
        let items = vec![Item {
            name: "f".into(),
            path: PathBuf::from("/f"),
            size: Some(100),
            item_type: ItemType::File,
            target_description: None,
            safety_level: crate::core::safety::SafetyLevel::Safe,
            is_shared: false,
        }];
        ds.push(PathBuf::from("/dir"), items, 0);
        // Cache has a different value — stack should win
        ds.cache_selection(
            PathBuf::from("/f"),
            (Some(999), crate::core::safety::SafetyLevel::Warn),
        );
        let meta = ds.lookup_meta(Path::new("/f")).unwrap();
        assert_eq!(meta.0, Some(100));
        assert_eq!(meta.1, crate::core::safety::SafetyLevel::Safe);
    }

    #[test]
    fn drill_state_breadcrumb_parts() {
        let mut ds = DrillState::new();
        ds.push(PathBuf::from("/a/dir1"), vec![], 0);
        ds.push(PathBuf::from("/a/dir1/sub"), vec![], 0);
        let parts = ds.breadcrumb_parts();
        assert_eq!(parts, vec!["dir1", "sub"]);
    }

    #[test]
    fn drill_state_collect_item_sizes() {
        let mut ds = DrillState::new();
        let items = vec![
            Item {
                name: "a".into(),
                path: PathBuf::from("/a"),
                size: Some(10),
                item_type: ItemType::File,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
            },
            Item {
                name: "b".into(),
                path: PathBuf::from("/b"),
                size: None,
                item_type: ItemType::File,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
            },
        ];
        ds.push(PathBuf::from("/dir"), items, 0);
        let sizes = ds.collect_item_sizes();
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes[&PathBuf::from("/a")], 10);
    }
}
