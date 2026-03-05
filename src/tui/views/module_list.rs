// Module list view (main screen).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ModuleStatus, ScanStatus};
use crate::tui::widgets::{
    checkbox_str, flash_line, format_size, format_size_or_placeholder, keybinding_bar, module_icon,
    render_status_line, CheckState,
};

/// Sort module indices by size descending. 0 B modules sink to the bottom.
fn sort_modules(app: &App, indices: &mut [usize]) {
    indices.sort_by(|&a, &b| {
        let size_a = app.modules[a].total_size;
        let size_b = app.modules[b].total_size;

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
        match (size_b, size_a) {
            (Some(sb), Some(sa)) => sb.cmp(&sa),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(&b),
        }
    });
}

/// Navigable module indices — excludes 0 B modules so they are skipped
/// during keyboard navigation and selection.
pub fn sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len())
        .filter(|&i| matches_filter(&app.modules[i].module.name, &app.filter_query))
        .filter(|&i| app.modules[i].total_size != Some(0))
        .collect();
    sort_modules(app, &mut indices);
    indices
}

/// All module indices including 0 B — used for rendering the full list.
fn all_sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len())
        .filter(|&i| matches_filter(&app.modules[i].module.name, &app.filter_query))
        .collect();
    sort_modules(app, &mut indices);
    indices
}

/// Render the module list view (main screen).
pub fn render(app: &App, frame: &mut Frame) {
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

/// Spinner characters that cycle during scanning.
const SPINNER_CHARS: &[char] = &[
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];

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

fn render_module_table(app: &App, frame: &mut Frame, area: Rect) {
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
        let icon = module_icon(&ms.module.name);
        let is_empty = ms.total_size == Some(0);
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
                .filter(|item| {
                    app.selected_items.contains(&item.path)
                        || app.selected_items.iter().any(|p| p.starts_with(&item.path))
                })
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

        // Name cell with icon (no status emoji)
        let name_cell = Cell::from(Line::from(vec![
            Span::styled(format!("{} ", icon), text_style),
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
                    format_size_or_placeholder(ms.total_size),
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
    state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut state);
}

/// Sort module indices for testing visibility.
#[cfg(test)]
pub fn all_sorted_module_indices_for_test(app: &App) -> Vec<usize> {
    all_sorted_module_indices(app)
}

fn render_description_pane(app: &App, frame: &mut Frame, area: Rect) {
    let description = sorted_module_indices(app)
        .get(app.selected_index)
        .map(|&idx| app.modules[idx].module.description.as_str())
        .unwrap_or("");
    let line = Line::from(Span::styled(
        format!(" {}", description),
        app.theme.style_description(),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
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
        let sorted = sorted_module_indices(app);
        let total = app.modules.len();
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
                ("i", "info"),
                ("/", "filter"),
                ("c", "clean"),
                ("tab", "all items"),
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

    fn make_module(name: &str, size: u64) -> ModuleState {
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
                targets: vec![Target {
                    path: "~/x".to_string(),
                    description: None,
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
        let app = App::new_for_test(vec![]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
    }

    #[test]
    fn render_does_not_panic_with_modules() {
        let app = App::new_for_test(vec![
            make_module("docker", 5_000_000_000),
            make_module("npm-cache", 1_000_000_000),
        ]);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
    }
}
