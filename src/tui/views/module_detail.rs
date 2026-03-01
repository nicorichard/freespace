// Module detail view — shows individual items within a module.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType, ModuleStatus};
use crate::tui::widgets::{
    checkbox_str, format_size, format_size_or_placeholder, module_icon, CheckState,
};

/// Spinner characters that cycle during loading.
const SPINNER_CHARS: &[char] = &[
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];

/// Compute item indices sorted by size descending.
/// Items with known sizes sort before those still calculating (None).
pub fn sorted_item_indices(app: &App, module_idx: usize) -> Vec<usize> {
    let items = app.current_detail_items(module_idx);
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
            Constraint::Length(3), // Title bar
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Path bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0], module_idx);
    render_items_table(app, frame, chunks[1], module_idx);
    render_path_bar(app, frame, chunks[2], module_idx);
    render_status_bar(app, frame, chunks[3], module_idx);
}

fn render_title_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let icon = module_icon(&ms.module.name);

    let title_text = if app.drill_stack.is_empty() {
        let size_text = match &ms.status {
            ModuleStatus::Loading | ModuleStatus::Discovering => "calculating...".to_string(),
            ModuleStatus::Error(e) => format!("Error: {}", e),
            ModuleStatus::Ready => format_size_or_placeholder(ms.total_size),
        };
        format!(" {} {} \u{2014} {} ", icon, ms.module.name, size_text)
    } else {
        // Breadcrumb: module > dir1 > dir2
        let mut parts = vec![ms.module.name.clone()];
        for level in &app.drill_stack {
            let dir_name = level
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| level.path.display().to_string());
            parts.push(dir_name);
        }
        format!(" {} {} ", icon, parts.join(" > "))
    };

    let title = Paragraph::new(Line::from(vec![Span::styled(
        title_text,
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

fn render_items_table(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let items = app.current_detail_items(module_idx);
    let drilled = !app.drill_stack.is_empty();
    let ms = &app.modules[module_idx];

    if items.is_empty() {
        let msg = if drilled {
            "Empty directory."
        } else {
            match &ms.status {
                ModuleStatus::Loading | ModuleStatus::Discovering => "Scanning for items...",
                ModuleStatus::Error(_) => "Could not scan this module.",
                ModuleStatus::Ready => "No items found.",
            }
        };
        let content = Paragraph::new(msg).style(app.theme.style_normal()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(app.theme.style_border()),
        );
        frame.render_widget(content, area);
        return;
    }

    // When drilled in, show a loading indicator until all item sizes are known
    if drilled && items.iter().any(|item| item.size.is_none()) {
        let sized = items.iter().filter(|i| i.size.is_some()).count();
        let total = items.len();
        let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
        let loading_text = format!("{} Calculating sizes... {}/{}", spinner, sized, total);
        let content = Paragraph::new(loading_text)
            .style(app.theme.style_status_loading())
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(content, area);
        return;
    }

    let sorted = sorted_item_indices(app, module_idx);

    let rows: Vec<Row> = sorted
        .iter()
        .map(|&item_idx| {
            let item = &items[item_idx];

            // Selection checkbox
            let check_state = if app.selected_items.contains(&item.path) {
                CheckState::All
            } else {
                CheckState::None
            };
            let checkbox_cell = Cell::from(Span::styled(
                checkbox_str(&check_state),
                app.theme.style_normal(),
            ));

            // Item name with folder icon for directories
            let display_name = match item.item_type {
                ItemType::Directory => format!("\u{1f4c1} {}", item.name),
                ItemType::File => item.name.clone(),
            };
            let name_cell = Cell::from(Span::styled(display_name, app.theme.style_normal()));

            // Size cell
            let size_cell = match item.size {
                Some(size) => Cell::from(Span::styled(format_size(size), app.theme.style_size())),
                None => {
                    if drilled {
                        Cell::from(Span::styled(
                            "calculating...",
                            app.theme.style_status_loading(),
                        ))
                    } else {
                        match &ms.status {
                            ModuleStatus::Loading | ModuleStatus::Discovering => Cell::from(
                                Span::styled("calculating...", app.theme.style_status_loading()),
                            ),
                            _ => {
                                Cell::from(Span::styled("N/A \u{26a0}", app.theme.style_warning()))
                            }
                        }
                    }
                }
            };

            Row::new(vec![checkbox_cell, name_cell, size_cell])
        })
        .collect();

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
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_path_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let sorted = sorted_item_indices(app, module_idx);
    let items = app.current_detail_items(module_idx);

    let path_text = sorted
        .get(app.selected_index)
        .and_then(|&idx| items.get(idx))
        .map(|item| format!(" {}", item.path.display()))
        .unwrap_or_default();

    let line = Line::from(Span::styled(path_text, app.theme.style_status_loading()));
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let drilled = !app.drill_stack.is_empty();

    let line = if app.filter_active {
        // Active filter input mode
        Line::from(vec![
            Span::styled(" / ", app.theme.style_size()),
            Span::styled(&app.filter_query, app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])
    } else if !app.filter_query.is_empty() {
        // Filter is set but not being edited
        let sorted = sorted_item_indices(app, module_idx);
        let total = app.current_detail_items(module_idx).len();
        let shown = sorted.len();
        Line::from(vec![
            Span::styled(
                format!(" filter: \"{}\" ({}/{})  ", app.filter_query, shown, total),
                app.theme.style_size(),
            ),
            Span::styled("/ filter  Esc clear", app.theme.style_normal()),
        ])
    } else if drilled {
        // Drilled-in status bar
        Line::from(vec![Span::styled(
            " \u{2191}/\u{2193} navigate  Space select  a all  n none  o open  / filter  Enter drill  c clean  Backspace/Esc back  ? help  q quit ",
            app.theme.style_normal(),
        )])
    } else {
        // Default status bar
        Line::from(vec![Span::styled(
            " \u{2191}/\u{2193} navigate  Space select  a all  n none  o open  / filter  Enter drill  c clean  Esc back  ? help  q quit ",
            app.theme.style_normal(),
        )])
    };
    let status = Paragraph::new(line);
    frame.render_widget(status, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Item, ItemType, ModuleState, ModuleStatus};
    use crate::module::manifest::{Module, Target};
    use std::path::PathBuf;

    fn make_detail_app() -> App {
        let module = Module {
            name: "test-module".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            targets: vec![Target {
                path: Some("~/test".to_string()),
                name: None,
                indicator: None,
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
                },
                Item {
                    name: "small-file".to_string(),
                    path: PathBuf::from("/tmp/small-file"),
                    size: Some(1_000),
                    item_type: ItemType::File,
                },
            ],
            total_size: Some(5_000_001_000),
            status: ModuleStatus::Ready,
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
        };
        let mut app = App::new_for_test(vec![ms]);
        app.current_view = crate::app::View::ModuleDetail(0);
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame, 0)).unwrap();
    }
}
