// Module list view (main screen).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{App, ModuleStatus, ScanStatus};
use crate::tui::widgets::{format_size, format_size_or_placeholder};

/// Get an emoji icon for a module based on its name.
fn module_icon(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("xcode") {
        "\u{1f528}" // 🔨
    } else if lower.contains("npm") || lower.contains("yarn") || lower.contains("pnpm") {
        "\u{1f4e6}" // 📦
    } else if lower.contains("homebrew") || lower.contains("brew") {
        "\u{1f37a}" // 🍺
    } else if lower.contains("docker") {
        "\u{1f433}" // 🐳
    } else if lower.contains("cache") {
        "\u{1f5c2}" // 🗂
    } else {
        "\u{1f4c1}" // 📁
    }
}

/// Compute module indices sorted by total_size descending.
/// Modules with known sizes sort before those still calculating (None).
pub fn sorted_module_indices(app: &App) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..app.modules.len()).collect();
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
    indices
}

/// Render the module list view (main screen).
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Minimum width check
    if area.width < 80 {
        let msg = Paragraph::new("Terminal too narrow. Please resize to at least 80 columns.")
            .style(app.theme.style_warning());
        frame.render_widget(msg, area);
        return;
    }

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

            // Name cell with icon and status indicator
            let (status_char, status_style) = match &ms.status {
                ModuleStatus::Loading | ModuleStatus::Discovering => {
                    ("\u{27f3} ", app.theme.style_status_loading())
                }
                ModuleStatus::Error(_) => ("\u{26a0} ", app.theme.style_status_error()),
                ModuleStatus::Ready => ("\u{25cf} ", app.theme.style_status_ready()),
            };

            let name_cell = Cell::from(Line::from(vec![
                Span::raw(format!("{} ", icon)),
                Span::styled(&ms.module.name, app.theme.style_normal()),
                Span::raw(" "),
                Span::styled(status_char, status_style),
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

            Row::new(vec![name_cell, items_cell, size_cell])
        })
        .collect();

    let widths = [
        Constraint::Min(30),
        Constraint::Length(12),
        Constraint::Length(16),
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
    let status = Paragraph::new(Line::from(vec![Span::styled(
        " \u{2191}/\u{2193} navigate  Enter details  c clean  d dry-run  ? help  q quit ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(status, area);
}
