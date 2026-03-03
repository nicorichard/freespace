// Application state and event loop.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::config::AppConfig;
use crate::core::cleaner;
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
    /// Stack of drill-in levels for directory exploration.
    pub drill_stack: Vec<DrillLevel>,
    /// Saved selected_index for the module list so we can restore it on back navigation.
    pub module_list_index: usize,
}

impl App {
    /// Create a new App with default state, load modules, and start scanning.
    pub fn new(cli_module_dirs: Vec<String>, cli_search_dirs: Vec<String>) -> Self {
        let (modules, search_dirs) =
            Self::load_modules_and_config(cli_module_dirs, cli_search_dirs);

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
            drill_stack: Vec::new(),
            module_list_index: 0,
        }
    }

    /// Discover and load modules from all configured directories.
    /// Returns module states and expanded search_dirs paths.
    fn load_modules_and_config(
        cli_module_dirs: Vec<String>,
        cli_search_dirs: Vec<String>,
    ) -> (Vec<ModuleState>, Vec<PathBuf>) {
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
            .map(|module| ModuleState {
                module,
                items: Vec::new(),
                total_size: None,
                status: ModuleStatus::Loading,
            })
            .collect();

        (module_states, search_dirs)
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

            // Process any pending scan messages (non-blocking)
            self.process_scan_messages();

            // Increment tick counter for spinner animation
            self.tick_count = self.tick_count.wrapping_add(1);
        }

        Ok(())
    }

    /// Drain pending scan messages from the background scanner and update state.
    fn process_scan_messages(&mut self) {
        while let Ok(msg) = self.scan_rx.try_recv() {
            match msg {
                ScanMessage::ItemDiscovered { module_index, item } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Discovering;
                        ms.items.push(item);
                    }
                }
                ScanMessage::ItemSized {
                    module_index,
                    item_index,
                    size,
                } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        if let Some(item) = ms.items.get_mut(item_index) {
                            item.size = Some(size);
                        }
                        // Incrementally update total_size
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
                    // Update drill item size if the drill level still matches
                    if let Some(level) = self.drill_stack.get_mut(drill_depth) {
                        if let Some(item) = level.items.get_mut(item_index) {
                            item.size = Some(size);
                        }
                    }
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
        if let Some(level) = self.drill_stack.last() {
            &level.items
        } else {
            &self.modules[module_idx].items
        }
    }

    /// List directory children as Items (instant, sizes are None).
    fn enumerate_directory(path: &Path) -> Vec<Item> {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
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
        let items: Vec<PathBuf> = self.drill_stack[drill_depth]
            .items
            .iter()
            .map(|i| i.path.clone())
            .collect();
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
        let key = if modifiers.contains(KeyModifiers::CONTROL) {
            match key {
                KeyCode::Char('n') => KeyCode::Down,
                KeyCode::Char('p') => KeyCode::Up,
                _ => key,
            }
        } else {
            key
        };

        // If filter input is active, let navigation keys pass through
        // to the view handler (like fzf), handle everything else as filter input
        if self.filter_active {
            match key {
                KeyCode::Down | KeyCode::Up => {} // fall through to view handler
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

        let sorted = views::module_detail::sorted_item_indices(self, module_idx);
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
            // Toggle selection on highlighted item
            KeyCode::Char(' ') => {
                if let Some(&item_idx) = sorted.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    let path = items[item_idx].path.clone();
                    if !self.selected_items.remove(&path) {
                        self.selected_items.insert(path);
                    }
                }
            }
            // Select all visible items
            KeyCode::Char('a') => {
                let paths: Vec<PathBuf> = self
                    .current_detail_items(module_idx)
                    .iter()
                    .map(|item| item.path.clone())
                    .collect();
                for path in paths {
                    self.selected_items.insert(path);
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
                }
            }
            // Enter: drill into directory, or cleanup if at top level
            KeyCode::Enter => {
                if let Some(&item_idx) = sorted.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    if matches!(items[item_idx].item_type, ItemType::Directory) {
                        let path = items[item_idx].path.clone();
                        let children = Self::enumerate_directory(&path);
                        let parent_selected_index = self.selected_index;
                        self.drill_stack.push(DrillLevel {
                            path,
                            items: children,
                            parent_selected_index,
                        });
                        self.clear_filter();
                        self.selected_index = 0;
                        let depth = self.drill_stack.len() - 1;
                        self.spawn_drill_size_scan(depth);
                    }
                }
            }
            // c: cleanup
            KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.drill_stack.clear();
                    self.previous_view = self.current_view;
                    self.current_view = View::CleanupConfirm;
                    self.selected_index = 0;
                }
            }
            // Open in file manager
            KeyCode::Char('o') => {
                if let Some(&item_idx) = sorted.get(self.selected_index) {
                    let items = self.current_detail_items(module_idx);
                    Self::open_in_file_manager(&items[item_idx].path);
                }
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
                } else if let Some(level) = self.drill_stack.pop() {
                    self.selected_index = level.parent_selected_index;
                    self.clear_filter();
                } else {
                    self.clear_filter();
                    self.current_view = View::ModuleList;
                    self.selected_index = self.module_list_index;
                }
            }
            // Backspace: pop drill stack if drilled in, else go back to module list
            KeyCode::Backspace => {
                if let Some(level) = self.drill_stack.pop() {
                    self.selected_index = level.parent_selected_index;
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

        let result = if permanent {
            cleaner::delete_items(&paths)
        } else {
            cleaner::trash_items(&paths)
        };

        // Remove successfully deleted items from module state
        let succeeded: HashSet<PathBuf> = result.succeeded.into_iter().collect();

        for ms in &mut self.modules {
            ms.items.retain(|item| !succeeded.contains(&item.path));
            // Recalculate total size from remaining items
            let total: u64 = ms.items.iter().filter_map(|i| i.size).sum();
            ms.total_size = if ms.items.is_empty() {
                Some(0)
            } else {
                Some(total)
            };
        }

        // Clear selected items
        self.selected_items.clear();
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
            drill_stack: Vec::new(),
            module_list_index: 0,
        }
    }
}

/// Which view is currently displayed.
#[derive(Clone, Copy)]
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
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            targets: vec![Target {
                path: Some("~/test".to_string()),
                name: None,
                indicator: None,
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
            })
            .collect();
        ModuleState {
            module,
            items,
            total_size: Some(0), // recalculated by sort
            status: ModuleStatus::Ready,
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
}
