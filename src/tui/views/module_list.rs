// Module list view (main screen).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ModuleStatus, ScanStatus, SortMode};
use crate::tui::widgets::{checkbox_str, format_size, format_size_or_placeholder, module_icon, CheckState};

/// Compute module indices sorted according to the current sort mode.
pub fn sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len())
        .filter(|&i| matches_filter(&app.modules[i].module.name, &app.filter_query))
        .collect();
    match app.sort_mode {
        SortMode::Default => {
            // Insertion order — no sorting needed
        }
        SortMode::Alphabetical => {
            indices.sort_by(|&a, &b| {
                let name_a = app.modules[a].module.name.to_lowercase();
                let name_b = app.modules[b].module.name.to_lowercase();
                name_a.cmp(&name_b)
            });
        }
        SortMode::SizeDesc => {
            indices.sort_by(|&a, &b| {
                let size_a = app.modules[a].total_size;
                let size_b = app.modules[b].total_size;
                match (size_b, size_a) {
                    (Some(sb), Some(sa)) => sb.cmp(&sa),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a.cmp(&b),
                }
            });
        }
    }
    indices
}

/// Render the module list view (main screen).
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(1),   // Content
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0]);
    render_module_table(app, frame, chunks[1]);
    render_status_bar(app, frame, chunks[2]);
}

/// Spinner characters that cycle during scanning.
const SPINNER_CHARS: &[char] = &['\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280f}'];

fn render_title_bar(app: &App, frame: &mut Frame, area: Rect) {
    let total: u64 = app.modules.iter().filter_map(|m| m.total_size).sum();

    let title_spans = match &app.scan_status {
        ScanStatus::Scanning => {
            let total_modules = app.modules.len();
            let completed_modules = app.modules.iter().filter(|m| {
                matches!(m.status, ModuleStatus::Ready | ModuleStatus::Error(_))
            }).count();

            let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
            let progress_text = format!(
                " {} Scanning... {}/{} modules ",
                spinner, completed_modules, total_modules
            );

            let any_known = app.modules.iter().any(|m| m.total_size.is_some());
            if any_known {
                vec![
                    Span::styled(" Freespace ", app.theme.style_header()),
                    Span::styled(
                        progress_text,
                        app.theme.style_status_loading(),
                    ),
                    Span::styled(
                        format!(" {} ", format_size(total)),
                        app.theme.style_size(),
                    ),
                ]
            } else {
                vec![
                    Span::styled(" Freespace ", app.theme.style_header()),
                    Span::styled(
                        progress_text,
                        app.theme.style_status_loading(),
                    ),
                ]
            }
        }
        _ => {
            let any_known = app.modules.iter().any(|m| m.total_size.is_some());
            if any_known {
                vec![Span::styled(
                    format!(" Freespace \u{2014} {} total ", format_size(total)),
                    app.theme.style_header(),
                )]
            } else {
                vec![Span::styled(" Freespace ", app.theme.style_header())]
            }
        }
    };

    let title = Paragraph::new(Line::from(title_spans))
        .block(
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

    let sorted = sorted_module_indices(app);

    let rows: Vec<Row> = sorted
        .iter()
        .map(|&module_idx| {
            let ms = &app.modules[module_idx];
            let icon = module_icon(&ms.module.name);

            // Checkbox: compute selection state for this module
            let check_state = if ms.items.is_empty() {
                CheckState::None
            } else {
                let selected_count = ms
                    .items
                    .iter()
                    .filter(|item| app.selected_items.contains(&item.path))
                    .count();
                if selected_count == 0 {
                    CheckState::None
                } else if selected_count == ms.items.len() {
                    CheckState::All
                } else {
                    CheckState::Partial
                }
            };
            let checkbox_cell =
                Cell::from(Span::styled(checkbox_str(&check_state), app.theme.style_normal()));

            // Name cell with icon (no status emoji)
            let name_cell = Cell::from(Line::from(vec![
                Span::raw(format!("{} ", icon)),
                Span::styled(&ms.module.name, app.theme.style_normal()),
            ]));

            // Items count
            let items_cell = Cell::from(format!("{} items", ms.items.len()));

            // Size cell with appropriate styling
            let size_cell = match &ms.status {
                ModuleStatus::Loading | ModuleStatus::Discovering => {
                    Cell::from(Span::styled("calculating...", app.theme.style_status_loading()))
                }
                ModuleStatus::Error(e) => {
                    Cell::from(Span::styled(format!("\u{26a0} {}", e), app.theme.style_error()))
                }
                ModuleStatus::Ready => {
                    Cell::from(Span::styled(
                        format_size_or_placeholder(ms.total_size),
                        app.theme.style_size(),
                    ))
                }
            };

            Row::new(vec![checkbox_cell, name_cell, items_cell, size_cell])
        })
        .collect();

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(30),   // Name
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
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let line = if app.filter_active {
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
            Span::styled(
                "/ filter  Esc clear",
                app.theme.style_normal(),
            ),
        ])
    } else {
        // Default status bar
        let sort_label = app.sort_mode.label();
        Line::from(vec![Span::styled(
            format!(
                " \u{2191}/\u{2193} navigate  Space select  Enter details  s sort ({})  / filter  c clean  ? help  q quit ",
                sort_label
            ),
            app.theme.style_normal(),
        )])
    };
    let status = Paragraph::new(line);
    frame.render_widget(status, area);
}
