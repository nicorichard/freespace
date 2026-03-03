// Standalone mini-TUI for selecting modules during installation.

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use std::time::Duration;

use crate::module::installer::InstallError;
use crate::module::manifest::Module;
use crate::tui::theme::Theme;
use crate::tui::widgets::{
    checkbox_str, keybinding_bar, normalize_emacs_key, render_status_line, CheckState,
};

struct InstallSelectState {
    candidates: Vec<(String, String)>, // (name, description)
    selected: Vec<bool>,
    cursor: usize,
    theme: Theme,
}

enum KeyResult {
    Continue,
    Confirm,
    Cancel,
}

impl InstallSelectState {
    fn new(modules: &[(String, Module)]) -> Self {
        let candidates: Vec<(String, String)> = modules
            .iter()
            .map(|(dir, m)| (m.name.clone(), dir.clone()))
            .collect();
        let count = candidates.len();
        Self {
            candidates,
            selected: vec![true; count],
            cursor: 0,
            theme: Theme::default(),
        }
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> KeyResult {
        // Ctrl+C always cancels
        if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
            return KeyResult::Cancel;
        }

        // Normalize emacs keys (Ctrl+N -> Down, Ctrl+P -> Up, etc.)
        let code = normalize_emacs_key(code, modifiers);

        match code {
            KeyCode::Esc | KeyCode::Char('q') => KeyResult::Cancel,
            KeyCode::Enter => KeyResult::Confirm,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.candidates.is_empty() {
                    self.cursor = (self.cursor + 1) % self.candidates.len();
                }
                KeyResult::Continue
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.candidates.is_empty() {
                    self.cursor = (self.cursor + self.candidates.len() - 1) % self.candidates.len();
                }
                KeyResult::Continue
            }
            KeyCode::Char(' ') => {
                if !self.candidates.is_empty() {
                    self.selected[self.cursor] = !self.selected[self.cursor];
                }
                KeyResult::Continue
            }
            KeyCode::Char('a') => {
                self.selected.iter_mut().for_each(|s| *s = true);
                KeyResult::Continue
            }
            KeyCode::Char('n') => {
                self.selected.iter_mut().for_each(|s| *s = false);
                KeyResult::Continue
            }
            _ => KeyResult::Continue,
        }
    }

    fn selected_indices(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter(|(_, &s)| s)
            .map(|(i, _)| i)
            .collect()
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title bar
                Constraint::Min(1),    // Table
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Title bar
        let title = Paragraph::new(Line::from(Span::styled(
            " Select modules to install ",
            self.theme.style_header(),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(self.theme.style_border()),
        );
        frame.render_widget(title, chunks[0]);

        // Module table
        let rows: Vec<Row> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(i, (name, dir))| {
                let check = if self.selected[i] {
                    CheckState::All
                } else {
                    CheckState::None
                };
                let style = self.theme.style_normal();
                Row::new(vec![
                    Cell::from(Span::styled(checkbox_str(&check), style)),
                    Cell::from(Span::styled(name.as_str(), style)),
                    Cell::from(Span::styled(dir.as_str(), self.theme.style_description())),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(5),  // Checkbox
            Constraint::Min(20),    // Name
            Constraint::Length(30), // Dir name
        ];

        let table = Table::new(rows, widths)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.style_border()),
            )
            .style(self.theme.style_normal())
            .row_highlight_style(self.theme.style_selected().add_modifier(Modifier::BOLD))
            .highlight_symbol("\u{25b6} ");

        let mut state = TableState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(table, chunks[1], &mut state);

        // Status bar
        let selected_count = self.selected.iter().filter(|&&s| s).count();
        let total = self.candidates.len();
        let left = keybinding_bar(
            &[
                ("space", "toggle"),
                ("a", "all"),
                ("n", "none"),
                ("enter", "confirm"),
                ("esc", "cancel"),
            ],
            &self.theme,
        );
        // Combine keybinding bar with selection count
        let mut spans = left.spans;
        spans.push(Span::styled(" \u{2502} ", self.theme.style_border()));
        spans.push(Span::styled(
            format!("{}/{} selected", selected_count, total),
            self.theme.style_size(),
        ));
        render_status_line(frame, chunks[2], Line::from(spans), &self.theme);
    }
}

/// Run an interactive module selection screen.
///
/// Returns the indices of the selected modules, or `InstallError::Cancelled`
/// if the user presses Esc/q/Ctrl-C.
pub fn run_install_select(modules: &[(String, Module)]) -> Result<Vec<usize>, InstallError> {
    let mut state = InstallSelectState::new(modules);
    let mut terminal = crate::tui::init().map_err(|e| InstallError::Other(e.into()))?;

    let result = run_event_loop(&mut state, &mut terminal);

    // Always restore terminal
    let _ = crate::tui::restore();

    result
}

fn run_event_loop(
    state: &mut InstallSelectState,
    terminal: &mut crate::tui::Tui,
) -> Result<Vec<usize>, InstallError> {
    loop {
        terminal
            .draw(|frame| state.render(frame))
            .map_err(|e| InstallError::Other(e.into()))?;

        if event::poll(Duration::from_millis(250)).map_err(|e| InstallError::Other(e.into()))? {
            if let Event::Key(key) = event::read().map_err(|e| InstallError::Other(e.into()))? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match state.handle_key(key.code, key.modifiers) {
                    KeyResult::Continue => {}
                    KeyResult::Confirm => return Ok(state.selected_indices()),
                    KeyResult::Cancel => return Err(InstallError::Cancelled),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(n: usize) -> InstallSelectState {
        let candidates: Vec<(String, String)> = (0..n)
            .map(|i| (format!("mod-{}", i), format!("dir-{}", i)))
            .collect();
        let count = candidates.len();
        InstallSelectState {
            candidates,
            selected: vec![true; count],
            cursor: 0,
            theme: Theme::default(),
        }
    }

    #[test]
    fn navigate_down_wraps() {
        let mut s = make_state(3);
        s.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(s.cursor, 1);
        s.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(s.cursor, 2);
        s.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(s.cursor, 0); // wraps
    }

    #[test]
    fn navigate_up_wraps() {
        let mut s = make_state(3);
        s.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(s.cursor, 2); // wraps to end
        s.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn toggle_selection() {
        let mut s = make_state(3);
        assert!(s.selected[0]);
        s.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(!s.selected[0]);
        s.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert!(s.selected[0]);
    }

    #[test]
    fn select_all() {
        let mut s = make_state(3);
        s.selected = vec![false; 3];
        s.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(s.selected.iter().all(|&v| v));
    }

    #[test]
    fn select_none() {
        let mut s = make_state(3);
        s.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(s.selected.iter().all(|&v| !v));
    }

    #[test]
    fn confirm_returns_selected() {
        let mut s = make_state(3);
        s.selected = vec![true, false, true];
        match s.handle_key(KeyCode::Enter, KeyModifiers::NONE) {
            KeyResult::Confirm => {
                assert_eq!(s.selected_indices(), vec![0, 2]);
            }
            _ => panic!("expected Confirm"),
        }
    }

    #[test]
    fn cancel_on_esc() {
        let mut s = make_state(3);
        match s.handle_key(KeyCode::Esc, KeyModifiers::NONE) {
            KeyResult::Cancel => {}
            _ => panic!("expected Cancel"),
        }
    }

    #[test]
    fn cancel_on_q() {
        let mut s = make_state(3);
        match s.handle_key(KeyCode::Char('q'), KeyModifiers::NONE) {
            KeyResult::Cancel => {}
            _ => panic!("expected Cancel"),
        }
    }

    #[test]
    fn cancel_on_ctrl_c() {
        let mut s = make_state(3);
        match s.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL) {
            KeyResult::Cancel => {}
            _ => panic!("expected Cancel"),
        }
    }

    #[test]
    fn ctrl_n_moves_down() {
        let mut s = make_state(3);
        s.handle_key(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert_eq!(s.cursor, 1);
        // Verify it did NOT deselect (the 'n' key bug)
        assert!(s.selected.iter().all(|&v| v));
    }

    #[test]
    fn ctrl_p_moves_up() {
        let mut s = make_state(3);
        s.handle_key(KeyCode::Char('j'), KeyModifiers::NONE); // move to 1
        s.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn empty_candidates_no_panic() {
        let mut s = make_state(0);
        s.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        s.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        s.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(s.selected_indices(), vec![]);
    }

    #[test]
    fn render_does_not_panic() {
        let s = make_state(3);
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| s.render(frame)).unwrap();
    }

    #[test]
    fn render_empty_does_not_panic() {
        let s = make_state(0);
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| s.render(frame)).unwrap();
    }
}
