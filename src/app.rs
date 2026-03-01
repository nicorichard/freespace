// Application state and event loop.

use std::collections::HashSet;
use std::path::PathBuf;
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
    pub config: AppConfig,
    pub theme: Theme,
    /// View to return to when leaving an overlay (CleanupConfirm, Help).
    pub previous_view: View,
    /// Whether the application should exit.
    pub should_quit: bool,
    /// Receiver for scan messages from the background scanner.
    scan_rx: mpsc::UnboundedReceiver<ScanMessage>,
    /// Counter incremented each event loop tick, used for spinner animation.
    pub tick_count: usize,
}

impl App {
    /// Create a new App with default state, load built-in modules, and start scanning.
    pub fn new() -> Self {
        let modules = Self::load_modules();

        // Create channel for scan messages
        let (tx, rx) = mpsc::unbounded_channel();

        // Collect module manifests for the scanner
        let manifests: Vec<Module> = modules.iter().map(|ms| ms.module.clone()).collect();

        // Start background scan
        let scan_status = if manifests.is_empty() {
            ScanStatus::Complete
        } else {
            scanner::start_scan(manifests, tx);
            ScanStatus::Scanning
        };

        Self {
            modules,
            current_view: View::ModuleList,
            selected_index: 0,
            selected_items: HashSet::new(),
            scan_status,
            config: AppConfig::default(),
            theme: Theme::default(),
            previous_view: View::ModuleList,
            should_quit: false,
            scan_rx: rx,
            tick_count: 0,
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
                        // Recalculate total size from all items with known sizes
                        let total: u64 = ms.items.iter().filter_map(|i| i.size).sum();
                        ms.total_size = Some(total);
                    }
                }
                ScanMessage::ModuleComplete { module_index } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Ready;
                        let total: u64 = ms.items.iter().filter_map(|i| i.size).sum();
                        ms.total_size = Some(total);
                    }
                }
                ScanMessage::ModuleError { module_index, error } => {
                    if let Some(ms) = self.modules.get_mut(module_index) {
                        ms.status = ModuleStatus::Error(error);
                    }
                }
                ScanMessage::ScanComplete => {
                    self.scan_status = ScanStatus::Complete;
                }
            }
        }
    }

    /// Dispatch key events based on the current view.
    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Global: q or Ctrl+C quits from any view
        if key == KeyCode::Char('q')
            || (key == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
        {
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
            // Esc quits from the base screen
            KeyCode::Esc => {
                self.should_quit = true;
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
