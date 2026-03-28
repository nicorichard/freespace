// File browser view — standalone directory exploration (drill-in).

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType, View};
use crate::tui::widgets::{
    checkbox_str, cmp_size_desc, format_size, is_checkbox_click, module_icon,
    render_view_status_bar,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the file browser view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let sorted = sorted_item_indices(app);
    let count = sorted.len();

    match key {
        // Navigate
        KeyCode::Char('j') | KeyCode::Down => {
            if count > 0 {
                app.selected_index = (app.selected_index + 1) % count;
            }
        }
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
        // Home / g: jump to first item
        KeyCode::Home | KeyCode::Char('g') => {
            app.selected_index = 0;
        }
        // End / G: jump to last item
        KeyCode::End | KeyCode::Char('G') => {
            if count > 0 {
                app.selected_index = count - 1;
            }
        }
        // Toggle selection
        KeyCode::Char(' ') => {
            if let Some(&item_idx) = sorted.get(app.selected_index) {
                let items = app.drill.current_items().unwrap();
                let path = items[item_idx].path.clone();
                let meta = (items[item_idx].size, items[item_idx].safety_level);
                if !app.selected_items.remove(&path) {
                    app.selected_items.retain(|p| !p.starts_with(&path));
                    app.selected_items.insert(path.clone());
                    app.drill.cache_selection(path, meta);
                } else {
                    app.drill.uncache_selection(&path);
                }
            }
        }
        // Select all
        KeyCode::Char('a') => {
            if let Some(items) = app.drill.current_items() {
                let snapshot: Vec<_> = items
                    .iter()
                    .map(|item| (item.path.clone(), item.size, item.safety_level))
                    .collect();
                for (path, size, safety) in snapshot {
                    app.selected_items.retain(|p| !p.starts_with(&path));
                    app.selected_items.insert(path.clone());
                    app.drill.cache_selection(path, (size, safety));
                }
            }
        }
        // Deselect all
        KeyCode::Char('n') => {
            if let Some(items) = app.drill.current_items() {
                let paths: Vec<PathBuf> = items.iter().map(|item| item.path.clone()).collect();
                for path in paths {
                    app.selected_items.remove(&path);
                    app.drill.uncache_selection(&path);
                }
            }
        }
        // Enter: drill deeper into directory
        KeyCode::Enter => {
            if let Some(&item_idx) = sorted.get(app.selected_index) {
                let items = app.drill.current_items().unwrap();
                if matches!(items[item_idx].item_type, ItemType::Directory) {
                    let path = items[item_idx].path.clone();
                    let children =
                        App::enumerate_directory(&path, &app.protected_paths, app.enforce_scope);
                    let parent_selected_index = app.selected_index;
                    app.drill.push(path, children, parent_selected_index);
                    app.clear_filter();
                    app.selected_index = 0;
                    let depth = app.drill.depth() - 1;
                    app.spawn_drill_size_scan(depth);
                }
            }
        }
        // Cleanup
        KeyCode::Char('c') => {
            if !app.selected_items.is_empty() {
                app.previous_view = app.current_view;
                app.confirm_checked = app.selected_items.clone();
                app.set_view(View::CleanupConfirm);
                app.selected_index = 0;
            }
        }
        // Open in file manager
        KeyCode::Char('o') => {
            if let Some(&item_idx) = sorted.get(app.selected_index) {
                let items = app.drill.current_items().unwrap();
                App::open_in_file_manager(&items[item_idx].path);
            }
        }
        // Filter
        KeyCode::Char('/') => {
            app.filter_active = true;
            app.filter_query.clear();
            app.filter_cursor = 0;
            app.selected_index = 0;
        }
        // Help
        KeyCode::Char('?') => {
            app.previous_view = app.current_view;
            app.set_view(View::Help);
        }
        // Esc: clear filter → pop drill → return to origin
        KeyCode::Esc => {
            if !app.filter_query.is_empty() {
                app.clear_filter();
                app.selected_index = 0;
            } else if let Some(parent_idx) = app.drill.pop() {
                app.clear_filter();
                if app.drill.is_active() {
                    app.selected_index = parent_idx;
                } else {
                    app.return_from_file_browser(parent_idx);
                }
            } else {
                app.clear_filter();
                app.return_from_file_browser(0);
            }
        }
        // Backspace: pop drill or return to origin
        KeyCode::Backspace => {
            if let Some(parent_idx) = app.drill.pop() {
                app.clear_filter();
                if app.drill.is_active() {
                    app.selected_index = parent_idx;
                } else {
                    app.return_from_file_browser(parent_idx);
                }
            } else {
                app.clear_filter();
                app.return_from_file_browser(0);
            }
        }
        _ => {}
    }
}

/// Compute item indices sorted by size descending for the current drill level.
pub fn sorted_item_indices(app: &App) -> Vec<usize> {
    let items = app.drill.current_items();
    let items = match items {
        Some(items) => items,
        None => return Vec::new(),
    };
    let mut indices: Vec<usize> = (0..items.len())
        .filter(|&i| matches_filter(&items[i].name, &[], &app.filter_query))
        .collect();
    indices.sort_by(|&a, &b| cmp_size_desc(items[a].size, items[b].size).then(a.cmp(&b)));
    indices
}

/// Handle click events for the file browser view.
pub fn handle_click(app: &mut App, col: u16, row: u16, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let table_area = chunks[1];
    let content_top = table_area.y + 1;
    let content_height = table_area.height.saturating_sub(2) as usize;
    if row < content_top || content_height == 0 {
        return;
    }
    let clicked_visual_offset = (row - content_top) as usize;
    if clicked_visual_offset >= content_height {
        return;
    }

    let sorted = sorted_item_indices(app);
    let scroll_offset = app.view_offset;
    let clicked_pos = scroll_offset + clicked_visual_offset;
    if clicked_pos < sorted.len() {
        let on_checkbox = is_checkbox_click(col, table_area);
        app.selected_index = clicked_pos;
        if on_checkbox {
            app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        }
    }
}

/// Render the file browser view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let module_idx = app.browser_module_idx;

    if module_idx >= app.modules.len() {
        let msg = Paragraph::new("Module not found.").style(app.theme.style_error());
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar (breadcrumb)
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Path bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0], module_idx);
    render_items_table(app, frame, chunks[1]);
    render_path_bar(app, frame, chunks[2]);
    render_status_bar(app, frame, chunks[3], module_idx);
}

fn render_title_bar(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let icon = module_icon(&ms.module.name);

    // Breadcrumb: module > dir1 > dir2
    let mut parts = vec![ms.module.name.clone()];
    parts.extend(app.drill.breadcrumb_parts());
    let title_text = format!(" {} {} ", icon, parts.join(" > "));

    let lines = vec![Line::from(vec![Span::styled(
        title_text,
        app.theme.style_header(),
    )])];

    let title = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

fn render_items_table(app: &mut App, frame: &mut Frame, area: Rect) {
    let items = match app.drill.current_items() {
        Some(items) => items,
        None => {
            let content = Paragraph::new("No items.")
                .style(app.theme.style_normal())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(app.theme.style_border()),
                );
            frame.render_widget(content, area);
            return;
        }
    };

    if items.is_empty() {
        let content = Paragraph::new("Empty directory.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(content, area);
        return;
    }

    let display_order = sorted_item_indices(app);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    for (pos, &item_idx) in display_order.iter().enumerate() {
        if pos == app.selected_index {
            visual_selected = rows.len();
        }

        let item = &items[item_idx];

        let check_state = app.check_state(&item.path);
        let checkbox_cell = Cell::from(Span::styled(
            checkbox_str(&check_state),
            app.theme.style_normal(),
        ));

        let display_name = match item.item_type {
            ItemType::Directory => format!("\u{1f4c1} {}", item.name),
            ItemType::File => item.name.clone(),
        };
        let name_cell = Cell::from(Span::styled(display_name, app.theme.style_normal()));

        let size_cell = match item.size {
            Some(size) => Cell::from(Span::styled(format_size(size), app.theme.style_size())),
            None => Cell::from(Span::styled(
                "calculating...",
                app.theme.style_status_loading(),
            )),
        };

        rows.push(Row::new(vec![checkbox_cell, name_cell, size_cell]));
    }

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(30),    // Name
        Constraint::Length(16), // Size
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal())
        .row_highlight_style(app.theme.style_selected())
        .highlight_symbol("\u{25b6} ");

    let mut state = TableState::default();
    *state.offset_mut() = app.view_offset;
    state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut state);
    app.view_offset = state.offset();
}

fn render_path_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let items = app.drill.current_items().unwrap_or(&[]);
    let display_order = sorted_item_indices(app);

    let path_text = display_order
        .get(app.selected_index)
        .and_then(|&idx| items.get(idx))
        .map(|item| format!(" {}", item.path.display()))
        .unwrap_or_default();

    let line = Line::from(Span::styled(path_text, app.theme.style_status_loading()));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect, _module_idx: usize) {
    let items = app.drill.current_items().unwrap_or(&[]);
    let shown = sorted_item_indices(app).len();
    render_view_status_bar(
        frame,
        area,
        &app.theme,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        app.filter_active,
        &app.filter_query,
        app.has_structured_filter(),
        shown,
        items.len(),
        &[
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("o", "open"),
            ("/", "search"),
            ("f", "filter"),
            ("c", "clean"),
            ("esc", "back"),
            ("?", "help"),
            ("q", "quit"),
        ],
    );
}
