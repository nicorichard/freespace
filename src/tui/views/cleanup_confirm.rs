// Cleanup confirmation dialog — centered modal showing items to be deleted.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App};
use crate::core::safety::SafetyLevel;
use crate::tui::widgets::{
    format_size, format_size_or_placeholder, keybinding_bar, render_status_line,
};

/// Collect selected items across all modules into a flat list of (name, path_display, size, safety_level).
/// Also includes items selected during drill-in that aren't direct module items.
pub fn collect_selected_items(app: &App) -> Vec<(String, String, Option<u64>, SafetyLevel)> {
    let mut items: Vec<(String, String, Option<u64>, SafetyLevel)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for ms in &app.modules {
        for item in &ms.items {
            if app.selected_items.contains(&item.path) {
                seen.insert(item.path.clone());
                items.push((
                    item.name.clone(),
                    item.path.display().to_string(),
                    item.size,
                    item.safety_level,
                ));
            }
        }
    }

    // Include drill-in selections not found in module items
    for path in &app.selected_items {
        if !seen.contains(path) {
            let (found_size, found_safety) = app
                .drill
                .lookup_meta(path)
                .unwrap_or((None, SafetyLevel::Safe));
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            items.push((name, path.display().to_string(), found_size, found_safety));
        }
    }

    // Sort by size descending (known sizes first)
    items.sort_by(|a, b| match (b.2, a.2) {
        (Some(sb), Some(sa)) => sb.cmp(&sa),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });

    items
}

/// Return the number of confirm-list items visible after filtering.
pub fn filtered_confirm_item_count(app: &App) -> usize {
    let items = collect_selected_items(app);
    if app.filter_query.is_empty() {
        items.len()
    } else {
        items
            .iter()
            .filter(|(name, _, _, _)| matches_filter(name, &app.filter_query))
            .count()
    }
}

/// Render the cleanup confirmation view as a fullscreen view.
pub fn render(app: &App, frame: &mut Frame) {
    let dialog_area = frame.area();

    let all_items = collect_selected_items(app);
    let item_count = all_items.len();
    let total_size: u64 = all_items.iter().filter_map(|i| i.2).sum();
    let known_count = all_items.iter().filter(|i| i.2.is_some()).count();
    let warned_count = all_items
        .iter()
        .filter(|i| i.3 == SafetyLevel::Warn)
        .count();

    // Apply filter for display, but keep unfiltered totals for summary
    let filtered_items: Vec<_> = if app.filter_query.is_empty() {
        all_items
    } else {
        all_items
            .into_iter()
            .filter(|(name, _, _, _)| matches_filter(name, &app.filter_query))
            .collect()
    };

    // Layout inside the dialog: header, items list, summary, action bar
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with border
            Constraint::Min(3),    // Items table (scrollable)
            Constraint::Length(2), // Summary line
            Constraint::Length(1), // Action bar
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0]);
    render_items_list(app, frame, inner_chunks[1], &filtered_items);
    render_summary(
        app,
        frame,
        inner_chunks[2],
        item_count,
        total_size,
        known_count,
        warned_count,
    );
    render_action_bar(
        app,
        frame,
        inner_chunks[3],
        filtered_items.len(),
        item_count,
    );
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![Span::styled(
        " Cleanup Preview",
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn render_items_list(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    items: &[(String, String, Option<u64>, SafetyLevel)],
) {
    if items.is_empty() {
        let msg = Paragraph::new("No items selected.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(msg, area);
        return;
    }

    let rows: Vec<Row> = items
        .iter()
        .map(|(name, path, size, safety)| {
            let is_warned = *safety == SafetyLevel::Warn;
            let name_style = if is_warned {
                app.theme.style_warning()
            } else {
                app.theme.style_normal()
            };
            let name_cell = Cell::from(Span::styled(name.as_str(), name_style));

            // Show path (truncated if needed)
            let max_path_len = (area.width as usize).saturating_sub(30);
            let path_display = if path.len() > max_path_len && max_path_len > 3 {
                format!("...{}", &path[path.len() - (max_path_len - 3)..])
            } else {
                path.clone()
            };
            let path_cell = Cell::from(Span::styled(
                path_display,
                Style::default().fg(app.theme.border), // dimmer color for path
            ));

            let size_cell = Cell::from(Span::styled(
                format_size_or_placeholder(*size),
                app.theme.style_size(),
            ));

            let safety_cell = if is_warned {
                Cell::from(Span::styled("[!]", app.theme.style_warning()))
            } else {
                Cell::from("")
            };

            Row::new(vec![name_cell, path_cell, size_cell, safety_cell])
        })
        .collect();

    let widths = [
        Constraint::Length(20), // Name
        Constraint::Min(20),    // Path
        Constraint::Length(12), // Size
        Constraint::Length(10), // Safety
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal())
        .row_highlight_style(app.theme.style_selected());

    // Scroll the table based on selected_index
    let mut state = TableState::default();
    state.select(Some(app.selected_index.min(items.len().saturating_sub(1))));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_summary(
    app: &App,
    frame: &mut Frame,
    area: Rect,
    item_count: usize,
    total_size: u64,
    known_count: usize,
    warned_count: usize,
) {
    let size_text = format_size(total_size);
    let suffix = if known_count < item_count {
        format!(
            " ({} of {} items have known sizes)",
            known_count, item_count
        )
    } else {
        String::new()
    };

    let summary_text = format!(
        " {} item{} \u{2014} {} to reclaim{}",
        item_count,
        if item_count == 1 { "" } else { "s" },
        size_text,
        suffix,
    );

    let mut spans = vec![Span::styled(
        summary_text,
        app.theme.style_size().add_modifier(Modifier::BOLD),
    )];
    if warned_count > 0 {
        spans.push(Span::styled(
            format!(
                " [!] {} item{} in sensitive location \u{2014} review carefully before proceeding",
                warned_count,
                if warned_count == 1 { "" } else { "s" }
            ),
            app.theme.style_warning(),
        ));
    }

    let summary = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(summary, area);
}

fn render_action_bar(app: &App, frame: &mut Frame, area: Rect, shown: usize, total: usize) {
    let line = if app.filter_active {
        // Active filter input mode
        Line::from(vec![
            Span::styled(" / ", app.theme.style_size()),
            Span::styled(&app.filter_query, app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])
    } else if !app.filter_query.is_empty() {
        // Filter is set but not being edited
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
                ("t", "trash"),
                ("d", "delete"),
                ("n", "cancel"),
                ("/", "filter"),
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
    use crate::app::{Item, ItemType, ModuleState, ModuleStatus, View};
    use crate::module::manifest::{Module, Target};
    use std::path::PathBuf;

    fn make_confirm_app() -> App {
        let module = Module {
            id: "test".to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            targets: vec![Target {
                path: "~/test".to_string(),
                description: None,
            }],
        };
        let ms = ModuleState {
            module,
            items: vec![
                Item {
                    name: "big".to_string(),
                    path: PathBuf::from("/tmp/big"),
                    size: Some(5_000_000_000),
                    item_type: ItemType::Directory,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
                },
                Item {
                    name: "small".to_string(),
                    path: PathBuf::from("/tmp/small"),
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
        // Select both items
        app.selected_items.insert(PathBuf::from("/tmp/big"));
        app.selected_items.insert(PathBuf::from("/tmp/small"));
        app.current_view = View::CleanupConfirm;
        app.previous_view = View::ModuleList;
        app
    }

    #[test]
    fn collect_selected_items_returns_all() {
        let app = make_confirm_app();
        let items = collect_selected_items(&app);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn collect_selected_items_sorted_by_size_desc() {
        let app = make_confirm_app();
        let items = collect_selected_items(&app);
        // Largest first; tuple is (name, path, size, safety_level)
        assert_eq!(items[0].0, "big");
        assert_eq!(items[1].0, "small");
    }

    #[test]
    fn filtered_confirm_count_all() {
        let app = make_confirm_app();
        assert_eq!(filtered_confirm_item_count(&app), 2);
    }

    #[test]
    fn filtered_confirm_count_with_filter() {
        let mut app = make_confirm_app();
        app.filter_query = "big".to_string();
        assert_eq!(filtered_confirm_item_count(&app), 1);
    }

    #[test]
    fn render_does_not_panic() {
        let app = make_confirm_app();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&app, frame)).unwrap();
    }
}
