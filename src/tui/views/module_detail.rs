// Module detail view — shows individual items within a module.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType, ModuleStatus, View};
use crate::module::manifest::RiskLevel;
use crate::tui::widgets::{
    checkbox_str, cmp_size_desc, format_size, format_size_or_placeholder, is_checkbox_click,
    module_icon, render_view_status_bar,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the module detail view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let module_idx = match &app.current_view {
        View::ModuleDetail(idx) => *idx,
        _ => return,
    };

    if module_idx >= app.modules.len() {
        return;
    }

    let (display_order, group_boundaries) = display_order_item_indices(app, module_idx);
    let count = display_order.len();

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
        // Jump to next target group
        KeyCode::Char('l') | KeyCode::Right => {
            if !group_boundaries.is_empty() {
                // Find the next group boundary after current position
                if let Some(&next) = group_boundaries.iter().find(|&&b| b > app.selected_index) {
                    app.selected_index = next;
                }
            }
        }
        // Jump to previous target group
        KeyCode::Char('h') | KeyCode::Left => {
            if !group_boundaries.is_empty() {
                // Find the group boundary at or before current position,
                // then jump to the one before that (or stay if at first group)
                let current_group = group_boundaries
                    .iter()
                    .rposition(|&b| b <= app.selected_index);
                if let Some(gi) = current_group {
                    if app.selected_index > group_boundaries[gi] {
                        // Not at start of current group — jump to its start
                        app.selected_index = group_boundaries[gi];
                    } else if gi > 0 {
                        // At start of current group — jump to previous group
                        app.selected_index = group_boundaries[gi - 1];
                    }
                }
            }
        }
        // Toggle selection on highlighted item
        KeyCode::Char(' ') => {
            if let Some(&item_idx) = display_order.get(app.selected_index) {
                let path = app.modules[module_idx].items[item_idx].path.clone();
                if !app.selected_items.remove(&path) {
                    app.selected_items.retain(|p| !p.starts_with(&path));
                    app.selected_items.insert(path);
                }
            }
        }
        // Select all visible items
        KeyCode::Char('a') => {
            let paths: Vec<PathBuf> = app.modules[module_idx]
                .items
                .iter()
                .map(|item| item.path.clone())
                .collect();
            for path in paths {
                app.selected_items.retain(|p| !p.starts_with(&path));
                app.selected_items.insert(path);
            }
        }
        // Deselect all visible items
        KeyCode::Char('n') => {
            let paths: Vec<PathBuf> = app.modules[module_idx]
                .items
                .iter()
                .map(|item| item.path.clone())
                .collect();
            for path in paths {
                app.selected_items.remove(&path);
            }
        }
        // Enter: drill into directory via FileBrowser
        KeyCode::Enter => {
            if let Some(&item_idx) = display_order.get(app.selected_index) {
                let items = &app.modules[module_idx].items;
                if matches!(items[item_idx].item_type, ItemType::Directory) {
                    let path = items[item_idx].path.clone();
                    let children =
                        App::enumerate_directory(&path, &app.protected_paths, app.enforce_scope);
                    app.browser_origin = View::ModuleDetail(module_idx);
                    app.browser_module_idx = module_idx;
                    app.drill.push(path, children, app.selected_index);
                    app.clear_filter();
                    app.set_view(View::FileBrowser);
                    app.selected_index = 0;
                    let depth = app.drill.depth() - 1;
                    app.spawn_drill_size_scan(depth);
                }
            }
        }
        // c: cleanup
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
            if let Some(&item_idx) = display_order.get(app.selected_index) {
                App::open_in_file_manager(&app.modules[module_idx].items[item_idx].path);
            }
        }
        // Open info overlay for this module
        KeyCode::Char('i') => {
            app.previous_view = app.current_view;
            app.set_view(View::Info(module_idx));
        }
        // Enter filter mode
        KeyCode::Char('/') => {
            app.filter_active = true;
            app.filter_query.clear();
            app.filter_cursor = 0;
            app.selected_index = 0;
        }
        // Esc: clear filter or go back to ModuleList
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
        // Backspace: go back to module list
        KeyCode::Backspace => {
            app.clear_filter();
            app.set_view(View::ModuleList);
            app.selected_index = app.module_list_index;
        }
        // Open help overlay
        KeyCode::Char('?') => {
            app.previous_view = app.current_view;
            app.set_view(View::Help);
        }
        _ => {}
    }
}

/// Compute item indices sorted by size descending.
/// Items with known sizes sort before those still calculating (None).
pub fn sorted_item_indices(app: &App, module_idx: usize) -> Vec<usize> {
    let items = &app.modules[module_idx].items;
    let mut indices: Vec<usize> = (0..items.len())
        .filter(|&i| matches_filter(&items[i].name, &[], &app.filter_query))
        .collect();
    indices.sort_by(|&a, &b| cmp_size_desc(items[a].size, items[b].size).then(a.cmp(&b)));
    indices
}

/// Returns item indices in display order, and the group boundary positions
/// (index into the returned Vec where each group starts).
/// When groups are active: items are ordered by group (groups by aggregate size desc,
/// items within each group by size desc).
/// When not grouped: same as sorted_item_indices, no group boundaries.
pub fn display_order_item_indices(app: &App, module_idx: usize) -> (Vec<usize>, Vec<usize>) {
    let groups = grouped_item_indices(app, module_idx);
    if groups.len() > 1 {
        let mut order = Vec::new();
        let mut boundaries = Vec::new();
        for (_desc, group_indices) in &groups {
            boundaries.push(order.len());
            order.extend(group_indices);
        }
        return (order, boundaries);
    }
    (sorted_item_indices(app, module_idx), Vec::new())
}

/// Handle click events for the module detail view.
pub fn handle_click(app: &mut App, col: u16, row: u16, area: Rect, module_idx: usize) {
    if module_idx >= app.modules.len() {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(1),
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

    let (display_order, group_boundaries) = display_order_item_indices(app, module_idx);
    let has_groups = !group_boundaries.is_empty();
    let scroll_offset = app.view_offset;

    let clicked_pos = if !has_groups {
        let pos = scroll_offset + clicked_visual_offset;
        if pos >= display_order.len() {
            return;
        }
        Some(pos)
    } else {
        // Build mapping: visual_row -> Option<display_order_position>
        let groups = grouped_item_indices_for_click(app, module_idx);
        let mut visual_to_nav: Vec<Option<usize>> = Vec::new();

        for (group_pos, (_desc, restore, group_indices)) in groups.iter().enumerate() {
            visual_to_nav.push(None); // Header row
            if restore.is_some() {
                visual_to_nav.push(None); // Restore hint row
            }
            let group_start = group_boundaries[group_pos];
            for (i, _) in group_indices.iter().enumerate() {
                visual_to_nav.push(Some(group_start + i));
            }
        }

        let clicked_visual_row = scroll_offset + clicked_visual_offset;
        visual_to_nav
            .get(clicked_visual_row)
            .copied()
            .unwrap_or(None)
    };

    if let Some(pos) = clicked_pos {
        let on_checkbox = is_checkbox_click(col, table_area);
        app.selected_index = pos;
        if on_checkbox {
            app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        }
    }
}

/// Helper: get grouped item indices with restore hints for click mapping.
pub(crate) fn grouped_item_indices_for_click(
    app: &App,
    module_idx: usize,
) -> Vec<(Option<String>, Option<String>, Vec<usize>)> {
    let items = &app.modules[module_idx].items;
    let sorted = sorted_item_indices(app, module_idx);

    let mut groups: Vec<(Option<String>, Option<String>, Vec<usize>)> = Vec::new();
    let mut group_map: std::collections::HashMap<Option<String>, usize> =
        std::collections::HashMap::new();

    for &idx in &sorted {
        let desc = items[idx].target_description.clone();
        if let Some(&group_idx) = group_map.get(&desc) {
            groups[group_idx].2.push(idx);
        } else {
            let restore = items[idx].restore_steps.clone();
            group_map.insert(desc.clone(), groups.len());
            groups.push((desc, restore, vec![idx]));
        }
    }

    groups.sort_by(|a, b| {
        let size_a: u64 = a.2.iter().filter_map(|&i| items[i].size).sum();
        let size_b: u64 = b.2.iter().filter_map(|&i| items[i].size).sum();
        size_b.cmp(&size_a)
    });

    groups
}

/// Render the module detail view.
pub fn render(app: &mut App, frame: &mut Frame, module_idx: usize) {
    let area = frame.area();

    // Bounds check
    if module_idx >= app.modules.len() {
        let msg = Paragraph::new("Module not found.").style(app.theme.style_error());
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Title bar (with description)
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Description pane
            Constraint::Length(1), // Path bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0], module_idx);
    render_items_table(app, frame, chunks[1], module_idx);
    render_description_pane(app, frame, chunks[2], module_idx);
    render_path_bar(app, frame, chunks[3], module_idx);
    render_status_bar(app, frame, chunks[4], module_idx);
}

fn render_title_bar(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let icon = module_icon(&ms.module.name);

    let size_text = match &ms.status {
        ModuleStatus::Loading | ModuleStatus::Discovering => "calculating...".to_string(),
        ModuleStatus::Error(e) => format!("Error: {}", e),
        ModuleStatus::Ready => format_size_or_placeholder(ms.total_size),
    };
    let title_text = format!(" {} {} \u{2014} {} ", icon, ms.module.name, size_text);

    let lines = vec![
        Line::from(vec![Span::styled(title_text, app.theme.style_header())]),
        Line::from(vec![Span::styled(
            format!(" {}", ms.module.description),
            app.theme.style_description(),
        )]),
    ];

    let title = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

/// Group sorted item indices by target_description.
/// Returns Vec of (description, item_indices) groups. Items within each group
/// are already sorted by size descending. Groups are ordered by aggregate size descending.
fn grouped_item_indices(app: &App, module_idx: usize) -> Vec<(Option<String>, Vec<usize>)> {
    let items = &app.modules[module_idx].items;
    let sorted = sorted_item_indices(app, module_idx);

    // Collect items into groups by target_description
    let mut groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
    let mut group_map: std::collections::HashMap<Option<String>, usize> =
        std::collections::HashMap::new();

    for &idx in &sorted {
        let desc = items[idx].target_description.clone();
        if let Some(&group_idx) = group_map.get(&desc) {
            groups[group_idx].1.push(idx);
        } else {
            group_map.insert(desc.clone(), groups.len());
            groups.push((desc, vec![idx]));
        }
    }

    // Sort groups by aggregate size descending
    groups.sort_by(|a, b| {
        let size_a: u64 = a.1.iter().filter_map(|&i| items[i].size).sum();
        let size_b: u64 = b.1.iter().filter_map(|&i| items[i].size).sum();
        size_b.cmp(&size_a)
    });

    groups
}

fn render_items_table(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    if app.modules[module_idx].items.is_empty() {
        let msg = match &app.modules[module_idx].status {
            ModuleStatus::Loading | ModuleStatus::Discovering => "Scanning for items...",
            ModuleStatus::Error(_) => "Could not scan this module.",
            ModuleStatus::Ready => "No items found.",
        };
        let content = Paragraph::new(msg).style(app.theme.style_normal()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border()),
        );
        frame.render_widget(content, area);
        return;
    }

    // Compute indices before borrowing items, to avoid borrow conflicts.
    let (display_order, group_boundaries) = display_order_item_indices(app, module_idx);
    let has_groups = !group_boundaries.is_empty();
    let groups = if has_groups {
        Some(grouped_item_indices(app, module_idx))
    } else {
        None
    };

    let items = &app.modules[module_idx].items;
    let ms = &app.modules[module_idx];

    let header_style = app.theme.style_border().add_modifier(Modifier::BOLD);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    let build_item_row = |item_idx: usize| -> Row {
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
        let name_cell = if item.is_shared {
            Cell::from(Line::from(vec![
                Span::styled(display_name, app.theme.style_normal()),
                Span::styled(" (shared)", app.theme.style_description()),
            ]))
        } else {
            Cell::from(Span::styled(display_name, app.theme.style_normal()))
        };

        let size_cell = match item.size {
            Some(size) => Cell::from(Span::styled(format_size(size), app.theme.style_size())),
            None => match &ms.status {
                ModuleStatus::Loading | ModuleStatus::Discovering => Cell::from(Span::styled(
                    "calculating...",
                    app.theme.style_status_loading(),
                )),
                _ => Cell::from(Span::styled("N/A \u{26a0}", app.theme.style_warning())),
            },
        };

        Row::new(vec![checkbox_cell, name_cell, size_cell])
    };

    if has_groups {
        // Render with target group section headers
        let groups = groups.as_ref().unwrap();

        for (group_pos, (desc, group_indices)) in groups.iter().enumerate() {
            // Section header row
            let group_size: u64 = group_indices.iter().filter_map(|&i| items[i].size).sum();
            let label = desc.as_deref().unwrap_or("Other");

            // Get risk level from first item in the group
            let risk = group_indices
                .first()
                .map(|&i| items[i].risk_level)
                .unwrap_or_default();
            let restore_kind = group_indices
                .first()
                .map(|&i| items[i].restore_kind)
                .unwrap_or_default();
            let mut badges = Vec::new();
            if matches!(risk, RiskLevel::Medium | RiskLevel::High) {
                badges.push(format!("[{} risk]", risk));
            }
            if restore_kind == crate::module::manifest::RestoreKind::Manual {
                badges.push("[manual restore]".to_string());
            }
            let risk_badge = if badges.is_empty() {
                String::new()
            } else {
                format!(" {}", badges.join(" "))
            };

            let header_text = format!(
                "\u{2500}\u{2500} {}{} \u{2500}\u{2500} {} \u{2500}\u{2500}",
                label,
                risk_badge,
                format_size(group_size)
            );

            let header_spans = if matches!(risk, RiskLevel::Medium | RiskLevel::High) {
                Span::styled(
                    header_text,
                    app.theme.style_warning().add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(header_text, header_style)
            };
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(header_spans),
                Cell::from(""),
            ]));

            // Show restore hint if present
            if let Some(restore) = group_indices
                .first()
                .and_then(|&i| items[i].restore_steps.as_deref())
            {
                let restore_text = format!("   \u{21b3} Restore: {}", restore);
                rows.push(Row::new(vec![
                    Cell::from(""),
                    Cell::from(Span::styled(restore_text, app.theme.style_description())),
                    Cell::from(""),
                ]));
            }

            let group_start = group_boundaries[group_pos];
            for (i, &_item_idx) in group_indices.iter().enumerate() {
                let display_pos = group_start + i;
                if display_pos == app.selected_index {
                    visual_selected = rows.len();
                }
                rows.push(build_item_row(display_order[display_pos]));
            }
        }
    } else {
        // Flat rendering (single group)
        for (pos, &item_idx) in display_order.iter().enumerate() {
            if pos == app.selected_index {
                visual_selected = rows.len();
            }
            rows.push(build_item_row(item_idx));
        }
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

fn render_description_pane(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let (display_order, _) = display_order_item_indices(app, module_idx);
    let items = &app.modules[module_idx].items;

    let item = display_order
        .get(app.selected_index)
        .and_then(|&idx| items.get(idx));

    let description = item
        .and_then(|i| i.target_description.as_deref())
        .unwrap_or("");

    let mut spans = vec![Span::styled(
        format!(" {}", description),
        app.theme.style_description(),
    )];

    // Append restore/risk badges
    if let Some(item) = item {
        if matches!(item.risk_level, RiskLevel::Medium | RiskLevel::High) {
            spans.push(Span::styled(
                format!(" [{} risk]", item.risk_level),
                app.theme.style_warning(),
            ));
        }
        if item.restore_kind == crate::module::manifest::RestoreKind::Manual {
            spans.push(Span::styled(
                " [manual restore]",
                app.theme.style_description(),
            ));
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_path_bar(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let (display_order, _) = display_order_item_indices(app, module_idx);
    let items = &app.modules[module_idx].items;

    let path_text = display_order
        .get(app.selected_index)
        .and_then(|&idx| items.get(idx))
        .map(|item| format!(" {}", item.path.display()))
        .unwrap_or_default();

    let line = Line::from(Span::styled(path_text, app.theme.style_status_loading()));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let shown = sorted_item_indices(app, module_idx).len();
    let total = app.modules[module_idx].items.len();
    render_view_status_bar(
        frame,
        area,
        &app.theme,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        app.filter_active,
        &app.filter_query,
        shown,
        total,
        &[
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("o", "open"),
            ("i", "info"),
            ("/", "filter"),
            ("c", "clean"),
            ("esc", "back"),
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

    fn make_detail_app() -> App {
        let module = Module {
            id: "test-module".to_string(),
            name: "test-module".to_string(),
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
        let ms = ModuleState {
            module,
            items: vec![
                Item {
                    name: "large-dir".to_string(),
                    path: PathBuf::from("/tmp/large-dir"),
                    size: Some(5_000_000_000),
                    item_type: ItemType::Directory,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
                    restore_kind: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk_level: crate::module::manifest::RiskLevel::default(),
                },
                Item {
                    name: "small-file".to_string(),
                    path: PathBuf::from("/tmp/small-file"),
                    size: Some(1_000),
                    item_type: ItemType::File,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
                    restore_kind: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk_level: crate::module::manifest::RiskLevel::default(),
                },
            ],
            total_size: Some(5_000_001_000),
            status: ModuleStatus::Ready,
            manifest_path: None,
        };
        let mut app = App::new_for_test(vec![ms]);
        app.current_view = crate::app::View::ModuleDetail(0);
        app
    }

    #[test]
    fn sorted_items_by_size_descending() {
        let app = make_detail_app();
        let sorted = sorted_item_indices(&app, 0);
        assert_eq!(sorted.len(), 2);
        assert_eq!(app.modules[0].items[sorted[0]].name, "large-dir");
        assert_eq!(app.modules[0].items[sorted[1]].name, "small-file");
    }

    #[test]
    fn sorted_items_respects_filter() {
        let mut app = make_detail_app();
        app.filter_query = "large".to_string();
        let sorted = sorted_item_indices(&app, 0);
        assert_eq!(sorted.len(), 1);
        assert_eq!(app.modules[0].items[sorted[0]].name, "large-dir");
    }

    #[test]
    fn render_does_not_panic() {
        let mut app = make_detail_app();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame, 0)).unwrap();
    }

    #[test]
    fn render_does_not_panic_empty_items() {
        let module = Module {
            id: "empty".to_string(),
            name: "empty".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            tags: vec![],
            targets: vec![],
        };
        let ms = ModuleState {
            module,
            items: vec![],
            total_size: Some(0),
            status: ModuleStatus::Ready,
            manifest_path: None,
        };
        let mut app = App::new_for_test(vec![ms]);
        app.current_view = crate::app::View::ModuleDetail(0);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame, 0)).unwrap();
    }
}
