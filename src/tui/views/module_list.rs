// Module list view (main screen).

use std::collections::HashSet;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, matches_structured_filter, App, ModuleStatus, ScanStatus, View};
use crate::tui::widgets::{
    checkbox_str, cmp_size_desc, format_size, format_size_or_placeholder, is_checkbox_click,
    parse_hex_color, render_view_status_bar, CheckState, ICON_DEFAULT_MODULE, SPINNER_CHARS,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the module list view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let sorted = sorted_module_indices(app);
    let count = sorted.len();

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
        // Enter detail view for selected module
        KeyCode::Enter => {
            if let Some(&module_idx) = sorted.get(app.selected_index) {
                app.module_list_index = app.selected_index;
                app.clear_filter();
                app.set_view(View::ModuleDetail(module_idx));
                app.selected_index = 0;
            }
        }
        // Toggle selection for all items in the focused module
        KeyCode::Char(' ') => {
            if let Some(&module_idx) = sorted.get(app.selected_index) {
                let items = &app.modules[module_idx].items;
                let all_selected = !items.is_empty()
                    && items
                        .iter()
                        .all(|item| app.selected_items.contains(&item.path));
                if all_selected {
                    // Deselect all
                    for item in &app.modules[module_idx].items {
                        app.selected_items.remove(&item.path);
                    }
                } else {
                    // Select all
                    for item in &app.modules[module_idx].items {
                        app.selected_items.insert(item.path.clone());
                    }
                }
            }
        }
        // Select all items across all visible (filtered) modules
        KeyCode::Char('a') => {
            for &module_idx in &sorted {
                let paths: Vec<PathBuf> = app.modules[module_idx]
                    .items
                    .iter()
                    .map(|item| item.path.clone())
                    .collect();
                for path in paths {
                    app.selected_items.insert(path);
                }
            }
        }
        // Deselect all items across all visible (filtered) modules
        KeyCode::Char('n') => {
            for &module_idx in &sorted {
                let paths: Vec<PathBuf> = app.modules[module_idx]
                    .items
                    .iter()
                    .map(|item| item.path.clone())
                    .collect();
                for path in paths {
                    app.selected_items.remove(&path);
                }
            }
        }
        // Open help overlay
        KeyCode::Char('?') => {
            app.previous_view = app.current_view;
            app.set_view(View::Help);
        }
        // Transition to cleanup confirmation if items are selected
        KeyCode::Char('c') => {
            if !app.selected_items.is_empty() {
                app.previous_view = app.current_view;
                app.confirm_checked = app.selected_items.clone();
                app.set_view(View::CleanupConfirm);
                app.selected_index = 0;
            }
        }
        // Enter filter mode
        KeyCode::Char('/') => {
            app.filter_active = true;
            app.filter_query.clear();
            app.filter_cursor = 0;
            app.selected_index = 0;
        }
        // Open info overlay for the selected module
        KeyCode::Char('i') => {
            if let Some(&module_idx) = sorted.get(app.selected_index) {
                app.previous_view = app.current_view;
                app.set_view(View::Info(module_idx));
            }
        }
        // Switch to flat view
        KeyCode::Tab => {
            app.module_list_index = app.selected_index;
            app.clear_filter();
            app.set_view(View::FlatView);
            app.selected_index = 0;
        }
        // Esc: clear filter
        KeyCode::Esc => {
            if !app.filter_query.is_empty() {
                app.clear_filter();
                app.selected_index = 0;
            }
        }
        _ => {}
    }
}

/// Sort module indices by size descending. 0 B modules sink to the bottom.
fn sort_modules(app: &App, indices: &mut [usize]) {
    indices.sort_by(|&a, &b| {
        let size_a = filtered_module_size(app, a);
        let size_b = filtered_module_size(app, b);

        // 0 B items sink to the bottom
        let a_empty = size_a == Some(0);
        let b_empty = size_b == Some(0);
        if a_empty != b_empty {
            return if a_empty {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            };
        }

        // Sort by size descending
        cmp_size_desc(size_a, size_b).then(a.cmp(&b))
    });
}

/// Navigable module indices — excludes 0 B modules so they are skipped
/// during keyboard navigation and selection.
pub fn sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len())
        .filter(|&i| {
            matches_filter(
                &app.modules[i].module.name,
                &app.modules[i].module.tags,
                &app.filter_query,
            )
        })
        .filter(|&i| passes_structured_filter_module(app, i))
        .filter(|&i| filtered_module_size(app, i) != Some(0))
        .collect();
    sort_modules(app, &mut indices);
    indices
}

/// All module indices including 0 B — used for rendering the full list.
pub fn all_sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len())
        .filter(|&i| {
            matches_filter(
                &app.modules[i].module.name,
                &app.modules[i].module.tags,
                &app.filter_query,
            )
        })
        .filter(|&i| passes_structured_filter_module(app, i))
        .collect();
    sort_modules(app, &mut indices);
    indices
}

/// Check if a module has any item passing the structured filter, or is still loading.
fn passes_structured_filter_module(app: &App, idx: usize) -> bool {
    if !app.has_structured_filter() {
        return true;
    }
    let ms = &app.modules[idx];
    if ms.items.is_empty() {
        return true; // still scanning
    }
    ms.items.iter().any(|item| {
        matches_structured_filter(
            item.risk_level,
            item.restore_kind,
            &app.filter_risk,
            &app.filter_restore,
        )
    })
}

/// Compute filtered module size (sum of matching items only when structured filter is active).
pub fn filtered_module_size(app: &App, idx: usize) -> Option<u64> {
    let ms = &app.modules[idx];
    if !app.has_structured_filter() {
        return ms.total_size;
    }
    let mut total = 0u64;
    let mut any_sized = false;
    for item in &ms.items {
        if matches_structured_filter(
            item.risk_level,
            item.restore_kind,
            &app.filter_risk,
            &app.filter_restore,
        ) {
            if let Some(size) = item.size {
                total += size;
                any_sized = true;
            }
        }
    }
    if any_sized {
        Some(total)
    } else {
        ms.total_size
    }
}

/// Compute filtered deduped total across all modules (each unique path counted once).
fn filtered_deduped_total(app: &App) -> u64 {
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for ms in &app.modules {
        for item in &ms.items {
            if matches_structured_filter(
                item.risk_level,
                item.restore_kind,
                &app.filter_risk,
                &app.filter_restore,
            ) {
                if let Some(size) = item.size {
                    if seen.insert(&item.path) {
                        total += size;
                    }
                }
            }
        }
    }
    total
}

/// Handle click events for the module list view.
pub fn handle_click(app: &mut App, col: u16, row: u16, area: Rect) {
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
        return;
    }
    let clicked_visual_offset = (row - content_top) as usize;
    if clicked_visual_offset >= content_height {
        return;
    }

    let all_sorted = all_sorted_module_indices(app);
    let navigable = sorted_module_indices(app);

    let scroll_offset = app.view_offset;
    let clicked_all_idx = scroll_offset + clicked_visual_offset;

    if clicked_all_idx >= all_sorted.len() {
        return;
    }
    let clicked_module_idx = all_sorted[clicked_all_idx];
    if let Some(nav_pos) = navigable.iter().position(|&i| i == clicked_module_idx) {
        let on_checkbox = is_checkbox_click(col, table_area);
        app.selected_index = nav_pos;
        if on_checkbox {
            app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        }
    }
}

/// Render the module list view (main screen).
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
    render_module_table(app, frame, chunks[1]);
    render_description_pane(app, frame, chunks[2]);
    render_status_bar(app, frame, chunks[3]);
}

fn render_title_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let total = if app.has_structured_filter() {
        filtered_deduped_total(app)
    } else {
        app.deduped_total
    };

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
            let total_modules = app.modules.len();
            let completed_modules = app
                .modules
                .iter()
                .filter(|m| matches!(m.status, ModuleStatus::Ready | ModuleStatus::Error(_)))
                .count();

            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            let progress_text = format!(
                " {} Scanning... {}/{} modules ",
                spinner, completed_modules, total_modules
            );

            let any_known = app.modules.iter().any(|m| m.total_size.is_some());
            let mut spans = vec![Span::styled(" Freespace ", app.theme.style_header())];
            spans.extend(dry_run_spans.clone());
            spans.push(Span::styled(
                progress_text,
                app.theme.style_status_loading(),
            ));
            if any_known {
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
                    format!(" Freespace \u{2014} {} reclaimable ", format_size(total)),
                    app.theme.style_header(),
                )]
            } else {
                vec![Span::styled(" Freespace ", app.theme.style_header())]
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

fn render_module_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.modules.is_empty() {
        let content = Paragraph::new("No modules loaded.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(content, area);
        return;
    }

    let all_sorted = all_sorted_module_indices(app);
    let navigable = sorted_module_indices(app);

    // The currently selected module index (in app.modules), if any
    let selected_module = navigable.get(app.selected_index).copied();

    // Build rows, tracking the visual row that corresponds to the selected module.
    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    for &module_idx in &all_sorted {
        // Track which visual row is the selected module
        if Some(module_idx) == selected_module {
            visual_selected = rows.len();
        }

        let ms = &app.modules[module_idx];
        let icon = if app.icons_enabled {
            ms.module.icon.as_deref().unwrap_or(ICON_DEFAULT_MODULE)
        } else {
            ""
        };
        let display_size = filtered_module_size(app, module_idx);
        let is_empty = display_size == Some(0);
        let dim_style = app.theme.style_border(); // mid-gray for 0 B modules
        let text_style = if is_empty {
            dim_style
        } else {
            app.theme.style_normal()
        };

        // Checkbox: compute selection state for this module
        // An item counts as selected if it is directly selected OR has any selected child.
        let check_state = if ms.items.is_empty() {
            CheckState::None
        } else {
            let selected_count = ms
                .items
                .iter()
                .filter(|item| !matches!(app.check_state(&item.path), CheckState::None))
                .count();
            if selected_count == 0 {
                CheckState::None
            } else if selected_count == ms.items.len() {
                CheckState::All
            } else {
                CheckState::Partial
            }
        };
        let checkbox_cell = Cell::from(Span::styled(checkbox_str(&check_state), text_style));

        // Name cell with icon
        let icon_style = ms
            .module
            .icon_color
            .as_deref()
            .and_then(parse_hex_color)
            .map(|c| text_style.fg(c))
            .unwrap_or(text_style);
        let name_cell = Cell::from(Line::from(vec![
            Span::styled(format!("{} ", icon), icon_style),
            Span::styled(&ms.module.name, text_style),
        ]));

        // Items count
        let items_cell = Cell::from(Span::styled(
            format!("{} items", ms.items.len()),
            text_style,
        ));

        // Size cell with appropriate styling
        let size_cell = match &ms.status {
            ModuleStatus::Loading | ModuleStatus::Discovering => Cell::from(Span::styled(
                "calculating...",
                app.theme.style_status_loading(),
            )),
            ModuleStatus::Error(e) => Cell::from(Span::styled(
                format!("\u{26a0} {}", e),
                app.theme.style_error(),
            )),
            ModuleStatus::Ready => {
                let size_style = if is_empty {
                    dim_style
                } else {
                    app.theme.style_size()
                };
                Cell::from(Span::styled(
                    format_size_or_placeholder(display_size),
                    size_style,
                ))
            }
        };

        rows.push(Row::new(vec![
            checkbox_cell,
            name_cell,
            items_cell,
            size_cell,
        ]));
    }

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(30),    // Name
        Constraint::Length(12), // Items
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

/// Sort module indices for testing visibility.
#[cfg(test)]
pub fn all_sorted_module_indices_for_test(app: &App) -> Vec<usize> {
    all_sorted_module_indices(app)
}

fn render_description_pane(app: &mut App, frame: &mut Frame, area: Rect) {
    let selected = sorted_module_indices(app).get(app.selected_index).copied();
    let description = selected
        .map(|idx| app.modules[idx].module.description.as_str())
        .unwrap_or("");
    let mut spans = vec![Span::styled(
        format!(" {}", description),
        app.theme.style_description(),
    )];
    if let Some(idx) = selected {
        let tags = &app.modules[idx].module.tags;
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
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let shown = sorted_module_indices(app).len();
    let total = app.modules.len();
    render_view_status_bar(
        frame,
        area,
        &app.theme,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        app.filter_active,
        &app.filter_query,
        app.has_structured_filter(),
        shown,
        total,
        &[
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("i", "info"),
            ("/", "search"),
            ("f", "filter"),
            ("c", "clean"),
            ("tab", "all items"),
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

    fn make_module(name: &str, size: u64) -> ModuleState {
        let module = Module {
            id: name.to_string(),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            tags: vec![],
            icon: None,
            icon_color: None,
            targets: vec![Target {
                paths: vec!["~/test".to_string()],
                description: None,
                restore: crate::module::manifest::RestoreKind::default(),
                restore_steps: None,
                risk: crate::module::manifest::RiskLevel::default(),
            }],
        };
        ModuleState {
            module,
            items: vec![Item {
                name: "item".to_string(),
                path: PathBuf::from("/tmp/item"),
                size: Some(size),
                item_type: ItemType::Directory,
                target_description: None,
                safety_level: crate::core::safety::SafetyLevel::Safe,
                is_shared: false,
                restore_kind: crate::module::manifest::RestoreKind::default(),
                restore_steps: None,
                risk_level: crate::module::manifest::RiskLevel::default(),
            }],
            total_size: Some(size),
            status: ModuleStatus::Ready,
            manifest_path: None,
        }
    }

    #[test]
    fn sorted_excludes_zero_size() {
        let m = ModuleState {
            module: Module {
                id: "empty".to_string(),
                name: "empty".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: "tester".to_string(),
                platforms: vec!["macos".to_string()],
                tags: vec![],
                icon: None,
                icon_color: None,
                targets: vec![Target {
                    paths: vec!["~/x".to_string()],
                    description: None,
                    restore: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk: crate::module::manifest::RiskLevel::default(),
                }],
            },
            items: vec![],
            total_size: Some(0),
            status: ModuleStatus::Ready,
            manifest_path: None,
        };
        let app = App::new_for_test(vec![m]);
        let sorted = sorted_module_indices(&app);
        assert!(sorted.is_empty());
    }

    #[test]
    fn sorted_by_size_descending() {
        let app = App::new_for_test(vec![
            make_module("small", 1_000),
            make_module("large", 1_000_000),
            make_module("medium", 100_000),
        ]);
        let sorted = sorted_module_indices(&app);
        assert_eq!(sorted.len(), 3);
        // Largest first
        assert_eq!(app.modules[sorted[0]].total_size, Some(1_000_000));
        assert_eq!(app.modules[sorted[1]].total_size, Some(100_000));
        assert_eq!(app.modules[sorted[2]].total_size, Some(1_000));
    }

    #[test]
    fn sorted_respects_filter() {
        let mut app = App::new_for_test(vec![
            make_module("docker", 1_000_000),
            make_module("npm-cache", 500_000),
        ]);
        app.filter_query = "dock".to_string();
        let sorted = sorted_module_indices(&app);
        assert_eq!(sorted.len(), 1);
        assert_eq!(app.modules[sorted[0]].module.name, "docker");
    }

    #[test]
    fn render_does_not_panic_empty_modules() {
        let mut app = App::new_for_test(vec![]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn render_does_not_panic_with_modules() {
        let mut app = App::new_for_test(vec![
            make_module("docker", 5_000_000_000),
            make_module("npm-cache", 1_000_000_000),
        ]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }
}
