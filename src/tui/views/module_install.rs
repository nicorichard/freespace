// In-TUI module install picker view.
//
// Replaces the standalone install_select mini-TUI with an integrated view
// that reuses the main app's styling, hotkeys, and search/filter infrastructure.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, InstallPhase, View};
use crate::tui::widgets::{
    checkbox_str, is_checkbox_click, render_view_status_bar, CheckState, SPINNER_CHARS,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the module install picker view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let phase = match &app.install_state {
        Some(s) => match s.phase {
            InstallPhase::Cloning => InstallPhaseKey::Cloning,
            InstallPhase::Picking => InstallPhaseKey::Picking,
            InstallPhase::Installing => InstallPhaseKey::Installing,
            InstallPhase::Done => InstallPhaseKey::Done,
        },
        None => return,
    };

    match phase {
        InstallPhaseKey::Cloning | InstallPhaseKey::Installing => {
            // Only allow cancel during clone/install
            if matches!(key, KeyCode::Esc | KeyCode::Char('q')) {
                app.cancel_module_install();
            }
        }
        InstallPhaseKey::Done => match key {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                app.finish_module_install();
            }
            _ => {}
        },
        InstallPhaseKey::Picking => {
            let count = filtered_candidate_indices(app).len();

            match key {
                // Navigate down
                KeyCode::Char('j') | KeyCode::Down => {
                    if count > 0 {
                        app.selected_index = (app.selected_index + 1) % count;
                    }
                }
                // Navigate up
                KeyCode::Char('k') | KeyCode::Up => {
                    if count > 0 {
                        app.selected_index = if app.selected_index == 0 {
                            count - 1
                        } else {
                            app.selected_index - 1
                        };
                    }
                }
                // Page Down
                KeyCode::PageDown => {
                    if count > 0 {
                        app.selected_index = (app.selected_index + PAGE_SIZE).min(count - 1);
                    }
                }
                // Page Up
                KeyCode::PageUp => {
                    if count > 0 {
                        app.selected_index = app.selected_index.saturating_sub(PAGE_SIZE);
                    }
                }
                // Home / g
                KeyCode::Home | KeyCode::Char('g') => {
                    app.selected_index = 0;
                }
                // End / G
                KeyCode::End | KeyCode::Char('G') => {
                    if count > 0 {
                        app.selected_index = count - 1;
                    }
                }
                // Toggle selection
                KeyCode::Char(' ') => {
                    let filtered = filtered_candidate_indices(app);
                    if let Some(&ci) = filtered.get(app.selected_index) {
                        if let Some(state) = &mut app.install_state {
                            state.candidates[ci].checked = !state.candidates[ci].checked;
                        }
                    }
                }
                // Select all
                KeyCode::Char('a') => {
                    let filtered = filtered_candidate_indices(app);
                    if let Some(state) = &mut app.install_state {
                        for &ci in &filtered {
                            state.candidates[ci].checked = true;
                        }
                    }
                }
                // Deselect all
                KeyCode::Char('n') => {
                    let filtered = filtered_candidate_indices(app);
                    if let Some(state) = &mut app.install_state {
                        for &ci in &filtered {
                            state.candidates[ci].checked = false;
                        }
                    }
                }
                // Enter filter mode
                KeyCode::Char('/') => {
                    app.filter_active = true;
                    app.filter_query.clear();
                    app.filter_cursor = 0;
                    app.selected_index = 0;
                }
                // Confirm selection
                KeyCode::Enter => {
                    app.confirm_module_install();
                }
                // Cancel
                KeyCode::Char('q') => {
                    app.cancel_module_install();
                }
                KeyCode::Esc => {
                    if !app.filter_query.is_empty() {
                        app.clear_filter();
                        app.selected_index = 0;
                    } else {
                        app.cancel_module_install();
                    }
                }
                // Help
                KeyCode::Char('?') => {
                    app.previous_view = View::ModuleInstall;
                    app.set_view(View::Help);
                }
                _ => {}
            }
        }
    }
}

// Copy of phase to avoid borrow issues
enum InstallPhaseKey {
    Cloning,
    Picking,
    Installing,
    Done,
}

/// Return indices of candidates that match the current filter.
fn filtered_candidate_indices(app: &App) -> Vec<usize> {
    let state = match &app.install_state {
        Some(s) => s,
        None => return Vec::new(),
    };
    state
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| matches_filter(&c.module.name, &c.module.tags, &app.filter_query))
        .map(|(i, _)| i)
        .collect()
}

/// Handle click events for the module install picker view.
pub fn handle_click(app: &mut App, col: u16, row: u16, area: Rect) -> bool {
    let state = match &app.install_state {
        Some(s) if matches!(s.phase, InstallPhase::Picking) => s,
        _ => return false,
    };
    if state.candidates.is_empty() {
        return false;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(area);

    let table_area = chunks[1];
    let content_top = table_area.y + 1;
    let content_height = table_area.height.saturating_sub(2) as usize;
    if row < content_top || content_height == 0 {
        return false;
    }
    let clicked_visual_offset = (row - content_top) as usize;
    if clicked_visual_offset >= content_height {
        return false;
    }

    let filtered = filtered_candidate_indices(app);
    let clicked_idx = app.view_offset + clicked_visual_offset;
    if clicked_idx >= filtered.len() {
        return false;
    }

    let on_checkbox = is_checkbox_click(col, table_area);
    app.selected_index = clicked_idx;
    if on_checkbox {
        let ci = filtered[clicked_idx];
        if let Some(state) = &mut app.install_state {
            state.candidates[ci].checked = !state.candidates[ci].checked;
        }
    }
    true
}

/// Render the module install picker view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(1),    // Content
            Constraint::Length(2), // Description pane
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0]);
    render_content(app, frame, chunks[1]);
    render_description_pane(app, frame, chunks[2]);
    render_status_bar(app, frame, chunks[3]);
}

fn render_title_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    let title_text = match &state.phase {
        InstallPhase::Cloning => {
            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            format!(" {} Cloning {}... ", spinner, state.source_str)
        }
        InstallPhase::Picking => {
            format!(" Installing modules from {} ", state.source_str)
        }
        InstallPhase::Installing => {
            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            format!(" {} Installing... ", spinner)
        }
        InstallPhase::Done => " Installation complete ".to_string(),
    };

    let title = Paragraph::new(Line::from(Span::styled(
        title_text,
        app.theme.style_header(),
    )))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    match &state.phase {
        InstallPhase::Cloning => {
            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            let content = Paragraph::new(format!(
                "\n  {} Fetching modules from {}...",
                spinner, state.source_str
            ))
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
            frame.render_widget(content, area);
        }
        InstallPhase::Picking => {
            render_candidate_table(app, frame, area);
        }
        InstallPhase::Installing => {
            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            let content = Paragraph::new(format!("\n  {} Installing selected modules...", spinner))
                .style(app.theme.style_normal())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(app.theme.style_border()),
                );
            frame.render_widget(content, area);
        }
        InstallPhase::Done => {
            render_results(app, frame, area);
        }
    }
}

fn render_candidate_table(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    if state.candidates.is_empty() {
        let content = Paragraph::new("  No modules found in source.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(content, area);
        return;
    }

    let filtered = filtered_candidate_indices(app);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    for (visual_idx, &ci) in filtered.iter().enumerate() {
        if visual_idx == app.selected_index {
            visual_selected = rows.len();
        }

        let candidate = &state.candidates[ci];
        let text_style = app.theme.style_normal();

        // Checkbox
        let check = if candidate.checked {
            CheckState::All
        } else {
            CheckState::None
        };
        let checkbox_cell = Cell::from(Span::styled(checkbox_str(&check), text_style));

        // Name
        let name_cell = Cell::from(Span::styled(&*candidate.module.name, text_style));

        // Version
        let version_cell = Cell::from(Span::styled(
            format!("v{}", candidate.module.version),
            app.theme.style_description(),
        ));

        // Status indicator
        let status_text = if candidate.was_installed && candidate.checked {
            "installed"
        } else if candidate.was_installed && !candidate.checked {
            "will remove"
        } else if !candidate.was_installed && candidate.checked {
            "new"
        } else {
            ""
        };
        let status_style = if !candidate.checked && candidate.was_installed {
            app.theme.style_warning()
        } else if candidate.checked && !candidate.was_installed {
            app.theme.style_size()
        } else {
            app.theme.style_description()
        };
        let status_cell = Cell::from(Span::styled(status_text, status_style));

        rows.push(Row::new(vec![
            checkbox_cell,
            name_cell,
            version_cell,
            status_cell,
        ]));
    }

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(20),    // Name
        Constraint::Length(12), // Version
        Constraint::Length(14), // Status
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal())
        .row_highlight_style(app.theme.style_selected().add_modifier(Modifier::BOLD))
        .highlight_symbol("\u{25b6} ");

    let mut table_state = TableState::default();
    *table_state.offset_mut() = app.view_offset;
    table_state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut table_state);
    app.view_offset = table_state.offset();
}

fn render_results(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    let mut lines: Vec<Line> = vec![Line::from("")];
    for result in &state.results {
        lines.push(Line::from(Span::styled(
            format!("  {}", result),
            app.theme.style_normal(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press Enter to continue.",
        app.theme.style_description(),
    )));

    let content = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(content, area);
}

fn render_description_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    if !matches!(state.phase, InstallPhase::Picking) {
        frame.render_widget(Paragraph::new(""), area);
        return;
    }

    let filtered = filtered_candidate_indices(app);
    let description = filtered
        .get(app.selected_index)
        .map(|&ci| state.candidates[ci].module.description.as_str())
        .unwrap_or("");

    let mut spans = vec![Span::styled(
        format!(" {}", description),
        app.theme.style_description(),
    )];

    if let Some(&ci) = filtered.get(app.selected_index) {
        let tags = &state.candidates[ci].module.tags;
        if !tags.is_empty() {
            let tag_text = tags
                .iter()
                .map(|t| format!("[{}]", t))
                .collect::<Vec<_>>()
                .join(" ");
            spans.push(Span::styled(
                format!("  {}", tag_text),
                app.theme.style_border(),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let state = match &app.install_state {
        Some(s) => s,
        None => return,
    };

    if !matches!(state.phase, InstallPhase::Picking) {
        // Minimal status bar for non-picking phases
        let hint = match state.phase {
            InstallPhase::Cloning | InstallPhase::Installing => "[esc] cancel",
            InstallPhase::Done => "[enter] continue",
            _ => "",
        };
        let line = Line::from(Span::styled(format!(" {}", hint), app.theme.style_border()));
        crate::tui::widgets::render_status_line(frame, area, line, &app.theme, false);
        return;
    }

    let filtered = filtered_candidate_indices(app);
    let shown = filtered.len();
    let total = state.candidates.len();

    render_view_status_bar(
        frame,
        area,
        app,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        app.filter_active,
        &app.filter_query,
        false,
        shown,
        total,
        crate::tui::keybindings::MODULE_INSTALL,
        false,
    );
}
