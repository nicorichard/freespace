// Project folder picker — chooses the directories searched for build output.
//
// A dialog, so it carries only the keys the choice needs: movement, `␣` to
// toggle, `↵` to save, `esc` to back out.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, ProjectSetupState};
use crate::tui::widgets::{checkbox_str, keybinding_bar, CheckState};

/// What a keypress asks of the app once the picker's own state is updated.
enum Action {
    Stay,
    Save,
    Cancel,
    /// Commit the typed path, which needs the app to validate it.
    AddTyped(String),
}

/// Handle key events for the project folder picker.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let action = match app.project_setup.as_mut() {
        Some(state) if state.input.is_some() => input_key(state, key),
        Some(state) => list_key(state, key),
        None => return,
    };

    match action {
        Action::Stay => {}
        Action::Save => app.save_project_setup(),
        Action::Cancel => app.cancel_project_setup(),
        Action::AddTyped(path) => match app.add_project_dir(&path) {
            Ok(()) => {
                if let Some(state) = app.project_setup.as_mut() {
                    state.input = None;
                    state.error = None;
                    state.cursor = state.candidates.len().saturating_sub(1);
                }
            }
            Err(message) => {
                if let Some(state) = app.project_setup.as_mut() {
                    state.error = Some(message);
                }
            }
        },
    }
}

/// Keys while navigating the list of folders.
fn list_key(state: &mut ProjectSetupState, key: KeyCode) -> Action {
    let rows = state.row_count();

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            state.cursor = (state.cursor + 1) % rows;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.cursor = if state.cursor == 0 {
                rows - 1
            } else {
                state.cursor - 1
            };
        }
        KeyCode::Home | KeyCode::Char('g') => state.cursor = 0,
        KeyCode::End | KeyCode::Char('G') => state.cursor = rows - 1,
        KeyCode::Char(' ') => match state.candidates.get_mut(state.cursor) {
            Some(candidate) => candidate.checked = !candidate.checked,
            None => state.start_input(),
        },
        KeyCode::Char('a') => {
            for candidate in &mut state.candidates {
                candidate.checked = true;
            }
        }
        KeyCode::Char('n') => {
            for candidate in &mut state.candidates {
                candidate.checked = false;
            }
        }
        KeyCode::Enter => {
            if state.on_add_row() {
                state.start_input();
            } else {
                return Action::Save;
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => return Action::Cancel,
        _ => {}
    }

    Action::Stay
}

/// Keys while a path is being typed into the "add another folder" row.
fn input_key(state: &mut ProjectSetupState, key: KeyCode) -> Action {
    let Some(input) = state.input.as_mut() else {
        return Action::Stay;
    };

    match key {
        KeyCode::Char(c) => input.push(c),
        KeyCode::Backspace => {
            input.pop();
        }
        KeyCode::Esc => {
            state.input = None;
            state.error = None;
        }
        KeyCode::Enter => {
            let typed = input.trim().to_string();
            if typed.is_empty() {
                state.input = None;
                state.error = None;
            } else {
                return Action::AddTyped(typed);
            }
        }
        _ => {}
    }

    Action::Stay
}

/// Render the picker as a centered dialog over the current view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let Some(state) = app.project_setup.as_ref() else {
        return;
    };

    let area = frame.area();
    let width = 68u16.min(area.width);
    // Intro (3) + blank + rows + error/blank + borders (2) + footer (1).
    let height = (state.row_count() as u16 + 8).min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Project folders ")
        .borders(Borders::ALL)
        .border_style(app.theme.style_border());
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " Where do you keep projects?",
            app.theme.style_header(),
        )),
        Line::from(Span::styled(
            " Each one is searched for node_modules, .next, .build and other",
            app.theme.style_description(),
        )),
        Line::from(Span::styled(
            " build output left behind in the projects inside it.",
            app.theme.style_description(),
        )),
        Line::from(""),
    ];

    for (i, candidate) in state.candidates.iter().enumerate() {
        let focused = state.cursor == i && state.input.is_none();
        let style = if focused {
            app.theme.style_selected()
        } else {
            app.theme.style_normal()
        };
        let check = if candidate.checked {
            CheckState::All
        } else {
            CheckState::None
        };
        let mut spans = vec![Span::styled(
            format!("  {} {}", checkbox_str(&check), candidate.path),
            style,
        )];
        if let Some(note) = candidate.note {
            spans.push(Span::styled(
                format!("  {}", note),
                app.theme.style_border(),
            ));
        }
        lines.push(Line::from(spans));
    }

    // The "add another folder" row, which becomes a text field once active.
    match &state.input {
        Some(input) => lines.push(Line::from(vec![
            Span::styled("      ", app.theme.style_normal()),
            Span::styled(input.as_str(), app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])),
        None => {
            let style = if state.on_add_row() {
                app.theme.style_selected()
            } else {
                app.theme.style_description()
            };
            lines.push(Line::from(Span::styled(
                "      + Add another folder\u{2026}",
                style,
            )));
        }
    }

    match &state.error {
        Some(error) => lines.push(Line::from(Span::styled(
            format!(" {}", error),
            app.theme.style_error(),
        ))),
        None => lines.push(Line::from("")),
    }

    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let bindings = if state.input.is_some() {
        crate::tui::keybindings::PROJECT_SETUP_INPUT
    } else {
        crate::tui::keybindings::PROJECT_SETUP
    };
    let footer = keybinding_bar(bindings, &app.theme, Some(app));
    frame.render_widget(Paragraph::new(footer), chunks[1]);
}
