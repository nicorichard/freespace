// Flat ranked view — all items from all modules sorted by size descending.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, matches_structured_filter, App, ItemType, ScanStatus, View};
use crate::tui::widgets::{
    checkbox_str, cmp_size_desc, format_size, is_checkbox_click, render_view_status_bar,
    SPINNER_CHARS,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the flat view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let sorted = sorted_flat_items(app);
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
            if let Some(&(module_idx, item_idx)) = sorted.get(app.selected_index) {
                let path = app.modules[module_idx].items[item_idx].path.clone();
                if !app.selected_items.remove(&path) {
                    app.selected_items.retain(|p| !p.starts_with(&path));
                    app.selected_items.insert(path);
                }
            }
        }
        // Select all visible items
        KeyCode::Char('a') => {
            for &(module_idx, item_idx) in &sorted {
                let path = app.modules[module_idx].items[item_idx].path.clone();
                app.selected_items.retain(|p| !p.starts_with(&path));
                app.selected_items.insert(path);
            }
        }
        // Deselect all visible items
        KeyCode::Char('n') => {
            for &(module_idx, item_idx) in &sorted {
                let path = app.modules[module_idx].items[item_idx].path.clone();
                app.selected_items.remove(&path);
            }
        }
        // Enter: drill into directory via FileBrowser
        KeyCode::Enter => {
            if let Some(&(module_idx, item_idx)) = sorted.get(app.selected_index) {
                let item = &app.modules[module_idx].items[item_idx];
                if matches!(item.item_type, ItemType::Directory) {
                    let path = item.path.clone();
                    let children =
                        App::enumerate_directory(&path, &app.protected_paths, app.enforce_scope);
                    app.browser_origin = View::FlatView;
                    app.browser_module_idx = module_idx;
                    app.flat_view_index = app.selected_index;
                    app.drill.push(path, children, 0);
                    app.set_view(View::FileBrowser);
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
            if let Some(&(module_idx, item_idx)) = sorted.get(app.selected_index) {
                App::open_in_file_manager(&app.modules[module_idx].items[item_idx].path);
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
        // Tab: switch back to module list
        KeyCode::Tab => {
            app.clear_filter();
            app.set_view(View::ModuleList);
            app.selected_index = app.module_list_index;
        }
        // Esc: clear filter or go back to module list
        KeyCode::Esc => {
            if !app.filter_query.is_empty() {
                app.clear_filter();
                app.selected_index = 0;
            } else {
                app.clear_filter();
                app.set_view(View::ModuleList);
                app.selected_index = app.module_list_index;
            }
        }
        _ => {}
    }
}

/// Returns a flat list of (module_index, item_index) pairs sorted by size descending.
/// Filters by the current filter query (matching item name or module name).
pub fn sorted_flat_items(app: &App) -> Vec<(usize, usize)> {
    let mut items: Vec<(usize, usize)> = Vec::new();

    for (mi, ms) in app.modules.iter().enumerate() {
        for (ii, item) in ms.items.iter().enumerate() {
            if !app.filter_query.is_empty() {
                // Match against item name OR module name
                if !matches_filter(&item.name, &[], &app.filter_query)
                    && !matches_filter(&ms.module.name, &ms.module.tags, &app.filter_query)
                {
                    continue;
                }
            }
            if !matches_structured_filter(
                item.risk_level,
                item.restore_kind,
                &app.filter_risk,
                &app.filter_restore,
            ) {
                continue;
            }
            items.push((mi, ii));
        }
    }

    items.sort_by(|&(am, ai), &(bm, bi)| {
        let size_a = app.modules[am].items[ai].size;
        let size_b = app.modules[bm].items[bi].size;
        cmp_size_desc(size_a, size_b).then_with(|| am.cmp(&bm).then(ai.cmp(&bi)))
    });

    items
}

/// Handle click events for the flat view.
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

    let flat_items = sorted_flat_items(app);
    let scroll_offset = app.view_offset;
    let clicked_pos = scroll_offset + clicked_visual_offset;
    if clicked_pos < flat_items.len() {
        let on_checkbox = is_checkbox_click(col, table_area);
        app.selected_index = clicked_pos;
        if on_checkbox {
            app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        }
    }
}

/// Render the flat ranked view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Path bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    let sorted = sorted_flat_items(app);

    render_title_bar(app, frame, chunks[0]);
    render_items_table(app, frame, chunks[1], &sorted);
    render_path_bar(app, frame, chunks[2], &sorted);
    render_status_bar(app, frame, chunks[3], &sorted);
}

fn render_title_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let total = app.deduped_total;

    let disk_suffix: Vec<Span> = match (app.disk_free, app.disk_total) {
        (Some(free), Some(total)) => vec![
            Span::styled("\u{2502} ", app.theme.style_header()),
            Span::styled(
                format!("{} free / {} ", format_size(free), format_size(total)),
                app.theme.style_header(),
            ),
        ],
        _ => vec![],
    };

    let dry_run_spans: Vec<Span> = if app.dry_run {
        vec![Span::styled(
            " [DRY RUN] ",
            app.theme.style_status_loading(),
        )]
    } else {
        vec![]
    };

    let title_spans = match &app.scan_status {
        ScanStatus::Scanning => {
            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            let mut spans = vec![Span::styled(
                " Freespace \u{2014} All Items ",
                app.theme.style_header(),
            )];
            spans.extend(dry_run_spans);
            spans.push(Span::styled(
                format!(" {} Scanning... ", spinner),
                app.theme.style_status_loading(),
            ));
            if total > 0 {
                spans.push(Span::styled(
                    format!(" {} ", format_size(total)),
                    app.theme.style_size(),
                ));
            }
            spans.extend(disk_suffix);
            spans
        }
        _ => {
            let any_known = app.modules.iter().any(|m| m.total_size.is_some());
            let mut spans = if any_known {
                vec![Span::styled(
                    format!(
                        " Freespace \u{2014} All Items \u{2014} {} reclaimable ",
                        format_size(total)
                    ),
                    app.theme.style_header(),
                )]
            } else {
                vec![Span::styled(
                    " Freespace \u{2014} All Items ",
                    app.theme.style_header(),
                )]
            };
            spans.extend(dry_run_spans);
            spans.extend(disk_suffix);
            spans
        }
    };

    let title = Paragraph::new(Line::from(title_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

fn render_items_table(app: &mut App, frame: &mut Frame, area: Rect, sorted: &[(usize, usize)]) {
    if sorted.is_empty() {
        let msg = match &app.scan_status {
            ScanStatus::Scanning => "Scanning...",
            _ => "No items found.",
        };
        let content = Paragraph::new(msg).style(app.theme.style_normal()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border()),
        );
        frame.render_widget(content, area);
        return;
    }

    let rows: Vec<Row> = sorted
        .iter()
        .map(|&(module_idx, item_idx)| {
            let item = &app.modules[module_idx].items[item_idx];
            let module_name = &app.modules[module_idx].module.name;

            // Selection checkbox
            let check_state = app.check_state(&item.path);
            let checkbox_cell = Cell::from(Span::styled(
                checkbox_str(&check_state),
                app.theme.style_normal(),
            ));

            // Item name with folder icon
            let display_name = match item.item_type {
                ItemType::Directory => format!("\u{1f4c1} {}", item.name),
                ItemType::File => item.name.clone(),
            };
            let name_cell = Cell::from(Span::styled(display_name, app.theme.style_normal()));

            // Module name (dimmed)
            let module_cell = Cell::from(Span::styled(
                module_name.as_str(),
                app.theme.style_description(),
            ));

            // Size
            let size_cell = match item.size {
                Some(size) => Cell::from(Span::styled(format_size(size), app.theme.style_size())),
                None => Cell::from(Span::styled(
                    "calculating...",
                    app.theme.style_status_loading(),
                )),
            };

            Row::new(vec![checkbox_cell, name_cell, module_cell, size_cell])
        })
        .collect();

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(25),    // Name
        Constraint::Length(20), // Module
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
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
    app.view_offset = state.offset();
}

fn render_path_bar(app: &mut App, frame: &mut Frame, area: Rect, sorted: &[(usize, usize)]) {
    let path_text = sorted
        .get(app.selected_index)
        .map(|&(mi, ii)| format!(" {}", app.modules[mi].items[ii].path.display()))
        .unwrap_or_default();

    let line = Line::from(Span::styled(path_text, app.theme.style_status_loading()));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect, sorted: &[(usize, usize)]) {
    let total: usize = app.modules.iter().map(|m| m.items.len()).sum();
    render_view_status_bar(
        frame,
        area,
        &app.theme,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        app.filter_active,
        &app.filter_query,
        app.has_structured_filter(),
        sorted.len(),
        total,
        &[
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("o", "open"),
            ("/", "search"),
            ("f", "filter"),
            ("c", "clean"),
            ("tab", "modules"),
            ("?", "help"),
            ("q", "quit"),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Item, ItemType, ModuleState, ModuleStatus};
    use crate::module::manifest::{Module, Target};
    use std::path::PathBuf;

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
        let items: Vec<Item> = items
            .into_iter()
            .map(|(name, size)| Item {
                name: name.to_string(),
                path: PathBuf::from(format!("/tmp/{}/{}", module.name, name)),
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
        let total: u64 = items.iter().filter_map(|i| i.size).sum();
        ModuleState {
            module,
            items,
            total_size: Some(total),
            status: ModuleStatus::Ready,
            manifest_path: None,
        }
    }

    #[test]
    fn sorted_flat_items_by_size() {
        let app = App::new_for_test(vec![
            make_module("docker", vec![("images", 5_000_000_000)]),
            make_module("npm", vec![("_cacache", 1_000_000_000)]),
        ]);
        let sorted = sorted_flat_items(&app);
        assert_eq!(sorted.len(), 2);
        // Largest first
        assert_eq!(app.modules[sorted[0].0].items[sorted[0].1].name, "images");
        assert_eq!(app.modules[sorted[1].0].items[sorted[1].1].name, "_cacache");
    }

    #[test]
    fn sorted_flat_items_respects_filter() {
        let mut app = App::new_for_test(vec![
            make_module("docker", vec![("images", 5_000_000_000)]),
            make_module("npm", vec![("_cacache", 1_000_000_000)]),
        ]);
        app.filter_query = "dock".to_string();
        let sorted = sorted_flat_items(&app);
        assert_eq!(sorted.len(), 1);
        assert_eq!(app.modules[sorted[0].0].module.name, "docker");
    }

    #[test]
    fn render_does_not_panic() {
        let mut app = App::new_for_test(vec![
            make_module("docker", vec![("images", 5_000_000_000)]),
            make_module("npm", vec![("_cacache", 1_000_000_000)]),
        ]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn render_does_not_panic_empty() {
        let mut app = App::new_for_test(vec![]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }
}
