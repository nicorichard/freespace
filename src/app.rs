// Application state and event loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::config::AppConfig;
use crate::core::cleaner;
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
    pub config: AppConfig,
    pub theme: Theme,
    /// View to return to when leaving an overlay (CleanupConfirm, Help).
    pub previous_view: View,
    /// Whether the application should exit.
    pub should_quit: bool,
}

impl App {
    /// Create a new App with default state and load built-in modules.
    pub fn new() -> Self {
        let modules = Self::load_modules();

        Self {
            modules,
            current_view: View::ModuleList,
            selected_index: 0,
            selected_items: HashSet::new(),
            scan_status: ScanStatus::Idle,
            config: AppConfig::default(),
            theme: Theme::default(),
            previous_view: View::ModuleList,
            should_quit: false,
        }
    }

    /// Discover and load built-in modules from the modules/ directory.
    fn load_modules() -> Vec<ModuleState> {
        let modules_dir = match manager::find_modules_dir() {
            Some(dir) => dir,
            None => return Vec::new(),
        };

        let (modules, warnings) = manager::load_builtin_modules(&modules_dir);

        // Log warnings to stderr (they won't be visible in the TUI but are
        // available if the user redirects stderr)
        for warning in &warnings {
            eprintln!("warning: {}", warning);
        }

        modules
            .into_iter()
            .map(|module| ModuleState {
                module,
                items: Vec::new(),
                total_size: None,
                status: ModuleStatus::Loading,
            })
            .collect()
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
                            self.handle_key(key.code);
                        }
                    }
                    Event::Resize(_, _) => {
                        // Re-render happens at the top of the next loop iteration
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Dispatch key events based on the current view.
    fn handle_key(&mut self, key: KeyCode) {
        // Global: q quits from any view
        if key == KeyCode::Char('q') {
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

    // Stub key handlers for each view — will be implemented in later stories.

    fn handle_key_module_list(&mut self, key: KeyCode) {
        let count = self.modules.len();
        if count == 0 {
            return;
        }

        match key {
            // Navigate down
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected_index = (self.selected_index + 1) % count;
            }
            // Navigate up
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_index = if self.selected_index == 0 {
                    count - 1
                } else {
                    self.selected_index - 1
                };
            }
            // Enter detail view for selected module
            KeyCode::Enter => {
                let sorted = views::module_list::sorted_module_indices(self);
                if let Some(&module_idx) = sorted.get(self.selected_index) {
                    self.current_view = View::ModuleDetail(module_idx);
                    self.selected_index = 0;
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
                    let path = self.modules[module_idx].items[item_idx].path.clone();
                    if !self.selected_items.remove(&path) {
                        self.selected_items.insert(path);
                    }
                }
            }
            // Select all items in current module
            KeyCode::Char('a') => {
                for item in &self.modules[module_idx].items {
                    self.selected_items.insert(item.path.clone());
                }
            }
            // Deselect all items in current module
            KeyCode::Char('n') => {
                for item in &self.modules[module_idx].items {
                    self.selected_items.remove(&item.path);
                }
            }
            // Transition to cleanup confirmation (if items selected)
            KeyCode::Enter | KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.previous_view = self.current_view;
                    self.current_view = View::CleanupConfirm;
                    self.selected_index = 0;
                }
            }
            // Return to module list view
            KeyCode::Backspace | KeyCode::Esc => {
                self.current_view = View::ModuleList;
                self.selected_index = 0;
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
        match key {
            // Confirm cleanup
            KeyCode::Char('y') => {
                if self.config.dry_run {
                    // Dry-run mode: summary was already displayed, just return
                    self.current_view = self.previous_view;
                    self.selected_index = 0;
                } else {
                    // Live mode: perform actual deletion
                    self.perform_cleanup();
                    self.current_view = self.previous_view;
                    self.selected_index = 0;
                }
            }
            // Cancel and return to previous view
            KeyCode::Char('n') | KeyCode::Esc => {
                self.current_view = self.previous_view;
                self.selected_index = 0;
            }
            _ => {}
        }
    }

    /// Perform live cleanup: delete selected items and update module state.
    fn perform_cleanup(&mut self) {
        let paths: Vec<std::path::PathBuf> =
            self.selected_items.iter().cloned().collect();

        let result = cleaner::delete_items(&paths);

        // Remove successfully deleted items from module state
        let succeeded: HashSet<PathBuf> =
            result.succeeded.into_iter().collect();

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

    /// Render the appropriate view based on current_view.
    fn render(&self, frame: &mut ratatui::Frame) {
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
