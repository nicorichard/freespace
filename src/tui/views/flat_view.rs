// Flat ranked view — all items from all modules sorted by size descending.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType, ScanStatus};
use crate::tui::widgets::{
    checkbox_str, flash_line, format_size, keybinding_bar, render_status_line, CheckState,
};

/// Spinner characters that cycle during scanning.
const SPINNER_CHARS: &[char] = &[
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];

/// Returns a flat list of (module_index, item_index) pairs sorted by size descending.
/// Filters by the current filter query (matching item name or module name).
pub fn sorted_flat_items(app: &App) -> Vec<(usize, usize)> {
    let mut items: Vec<(usize, usize)> = Vec::new();

    for (mi, ms) in app.modules.iter().enumerate() {
        for (ii, item) in ms.items.iter().enumerate() {
            if !app.filter_query.is_empty() {
                // Match against item name OR module name
                if !matches_filter(&item.name, &app.filter_query)
                    && !matches_filter(&ms.module.name, &app.filter_query)
                {
                    continue;
                }
            }
            items.push((mi, ii));
        }
    }

    items.sort_by(|&(am, ai), &(bm, bi)| {
        let size_a = app.modules[am].items[ai].size;
        let size_b = app.modules[bm].items[bi].size;
        match (size_b, size_a) {
            (Some(sb), Some(sa)) => sb.cmp(&sa),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => am.cmp(&bm).then(ai.cmp(&bi)),
        }
    });

    items
}

/// Render the flat ranked view.
pub fn render(app: &App, frame: &mut Frame) {
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

    render_title_bar(app, frame, chunks[0]);
    render_items_table(app, frame, chunks[1]);
    render_path_bar(app, frame, chunks[2]);
    render_status_bar(app, frame, chunks[3]);
}

fn render_title_bar(app: &App, frame: &mut Frame, area: Rect) {
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

fn render_items_table(app: &App, frame: &mut Frame, area: Rect) {
    let sorted = sorted_flat_items(app);

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
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_path_bar(app: &App, frame: &mut Frame, area: Rect) {
    let sorted = sorted_flat_items(app);

    let path_text = sorted
        .get(app.selected_index)
        .map(|&(mi, ii)| format!(" {}", app.modules[mi].items[ii].path.display()))
        .unwrap_or_default();

    let line = Line::from(Span::styled(path_text, app.theme.style_status_loading()));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let line = if let Some((ref msg, ref level)) = app.flash_message {
        flash_line(msg, level, &app.theme)
    } else if app.filter_active {
        Line::from(vec![
            Span::styled(" / ", app.theme.style_size()),
            Span::styled(&app.filter_query, app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])
    } else if !app.filter_query.is_empty() {
        let sorted = sorted_flat_items(app);
        let total: usize = app.modules.iter().map(|m| m.items.len()).sum();
        let shown = sorted.len();
        Line::from(vec![
            Span::styled(
                format!(" filter: \"{}\" ({}/{})  ", app.filter_query, shown, total),
                app.theme.style_size(),
            ),
            Span::styled("/ filter  Esc clear", app.theme.style_normal()),
        ])
    } else {
        keybinding_bar(
            &[
                ("space", "select"),
                ("a", "all"),
                ("n", "none"),
                ("o", "open"),
                ("/", "filter"),
                ("c", "clean"),
                ("tab", "modules"),
                ("?", "help"),
                ("q", "quit"),
            ],
            &app.theme,
        )
    };
    render_status_line(frame, area, line, &app.theme);
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
            targets: vec![Target {
                path: "~/test".to_string(),
                description: None,
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
        let app = App::new_for_test(vec![
            make_module("docker", vec![("images", 5_000_000_000)]),
            make_module("npm", vec![("_cacache", 1_000_000_000)]),
        ]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
    }

    #[test]
    fn render_does_not_panic_empty() {
        let app = App::new_for_test(vec![]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
    }
}
