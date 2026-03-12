// Module detail view — shows individual items within a module.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType, ModuleStatus};
use crate::tui::widgets::{
    checkbox_str, flash_line, format_size, format_size_or_placeholder, keybinding_bar, module_icon,
    render_status_line, CheckState,
};

/// Compute item indices sorted by size descending.
/// Items with known sizes sort before those still calculating (None).
pub fn sorted_item_indices(app: &App, module_idx: usize) -> Vec<usize> {
    let items = &app.modules[module_idx].items;
    let mut indices: Vec<usize> = (0..items.len())
        .filter(|&i| matches_filter(&items[i].name, &app.filter_query))
        .collect();
    indices.sort_by(|&a, &b| {
        let size_a = items[a].size;
        let size_b = items[b].size;
        match (size_b, size_a) {
            (Some(sb), Some(sa)) => sb.cmp(&sa),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(&b),
        }
    });
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

/// Render the module detail view.
pub fn render(app: &App, frame: &mut Frame, module_idx: usize) {
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

fn render_title_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
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

fn render_items_table(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let items = &app.modules[module_idx].items;
    let ms = &app.modules[module_idx];

    if items.is_empty() {
        let msg = match &ms.status {
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

    let (display_order, group_boundaries) = display_order_item_indices(app, module_idx);
    let has_groups = !group_boundaries.is_empty();

    let header_style = app.theme.style_border().add_modifier(Modifier::BOLD);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    let build_item_row = |item_idx: usize| -> Row {
        let item = &items[item_idx];

        let check_state = if app.selected_items.contains(&item.path)
            || app.selected_items.iter().any(|p| item.path.starts_with(p))
        {
            CheckState::All
        } else if app.selected_items.iter().any(|p| p.starts_with(&item.path)) {
            CheckState::Partial
        } else {
            CheckState::None
        };
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
        let groups = grouped_item_indices(app, module_idx);

        for (group_pos, (desc, group_indices)) in groups.iter().enumerate() {
            // Section header row
            let group_size: u64 = group_indices.iter().filter_map(|&i| items[i].size).sum();
            let label = desc.as_deref().unwrap_or("Other");
            let header_text = format!(
                "\u{2500}\u{2500} {} \u{2500}\u{2500} {} \u{2500}\u{2500}",
                label,
                format_size(group_size)
            );
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(Span::styled(header_text, header_style)),
                Cell::from(""),
            ]));

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
    state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_description_pane(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let (display_order, _) = display_order_item_indices(app, module_idx);
    let items = &app.modules[module_idx].items;

    let description = display_order
        .get(app.selected_index)
        .and_then(|&idx| items.get(idx))
        .and_then(|item| item.target_description.as_deref())
        .unwrap_or("");

    let line = Line::from(Span::styled(
        format!(" {}", description),
        app.theme.style_description(),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_path_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
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

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let line = if let Some((ref msg, ref level)) = app.flash_message {
        flash_line(msg, level, &app.theme)
    } else if app.filter_active {
        // Active filter input mode
        Line::from(vec![
            Span::styled(" / ", app.theme.style_size()),
            Span::styled(&app.filter_query, app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])
    } else if !app.filter_query.is_empty() {
        // Filter is set but not being edited
        let sorted = sorted_item_indices(app, module_idx);
        let total = app.modules[module_idx].items.len();
        let shown = sorted.len();
        Line::from(vec![
            Span::styled(
                format!(" filter: \"{}\" ({}/{})  ", app.filter_query, shown, total),
                app.theme.style_size(),
            ),
            Span::styled("/ filter  Esc clear", app.theme.style_normal()),
        ])
    } else {
        let mut bindings: Vec<(&str, &str)> = vec![
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("o", "open"),
            ("i", "info"),
        ];
        bindings.extend([
            ("/", "filter"),
            ("c", "clean"),
            ("esc", "back"),
            ("?", "help"),
            ("q", "quit"),
        ]);
        keybinding_bar(&bindings, &app.theme)
    };
    render_status_line(frame, area, line, &app.theme);
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
            targets: vec![Target {
                paths: vec!["~/test".to_string()],
                description: None,
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
                },
                Item {
                    name: "small-file".to_string(),
                    path: PathBuf::from("/tmp/small-file"),
                    size: Some(1_000),
                    item_type: ItemType::File,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
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
        let app = make_detail_app();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame, 0)).unwrap();
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
        terminal.draw(|frame| render(&app, frame, 0)).unwrap();
    }
}
