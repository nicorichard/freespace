// Application state and event loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::config::AppConfig;
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
                self.current_view = View::Help;
            }
            // Transition to cleanup confirmation if items are selected
            KeyCode::Char('c') => {
                if !self.selected_items.is_empty() {
                    self.current_view = View::CleanupConfirm;
                }
            }
            _ => {}
        }
    }

    fn handle_key_module_detail(&mut self, _key: KeyCode) {
        // Will be implemented in US-014
    }

    fn handle_key_cleanup_confirm(&mut self, _key: KeyCode) {
        // Will be implemented in US-016
    }

    fn handle_key_help(&mut self, _key: KeyCode) {
        // Will be implemented in US-017
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
        let area = frame.area();
        let placeholder = Paragraph::new("Cleanup confirmation — not yet implemented. Press q to quit.")
            .style(self.theme.style_normal())
            .block(
                Block::default()
                    .title(" Cleanup Preview ")
                    .borders(Borders::ALL)
                    .border_style(self.theme.style_border()),
            );
        frame.render_widget(placeholder, area);
    }

    fn render_help(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let placeholder = Paragraph::new("Help overlay — not yet implemented. Press q to quit.")
            .style(self.theme.style_normal())
            .block(
                Block::default()
                    .title(" Help ")
                    .borders(Borders::ALL)
                    .border_style(self.theme.style_border()),
            );
        frame.render_widget(placeholder, area);
    }
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
