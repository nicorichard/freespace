// Cleanup confirmation dialog — centered modal showing items to be deleted.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::App;
use crate::tui::widgets::{format_size, format_size_or_placeholder};

/// Collect selected items across all modules into a flat list of (name, path_display, size).
fn collect_selected_items(app: &App) -> Vec<(String, String, Option<u64>)> {
    let mut items: Vec<(String, String, Option<u64>)> = Vec::new();

    for ms in &app.modules {
        for item in &ms.items {
            if app.selected_items.contains(&item.path) {
                items.push((
                    item.name.clone(),
                    item.path.display().to_string(),
                    item.size,
                ));
            }
        }
    }

    // Sort by size descending (known sizes first)
    items.sort_by(|a, b| {
        match (b.2, a.2) {
            (Some(sb), Some(sa)) => sb.cmp(&sa),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.0.cmp(&b.0),
        }
    });

    items
}

/// Compute a centered rectangle that is at most `max_percent` of the terminal area,
/// with an absolute cap on width and height.
fn centered_rect(area: Rect, max_percent: u16) -> Rect {
    let max_width = area.width * max_percent / 100;
    let max_height = area.height * max_percent / 100;

    // Ensure minimum usable size
    let width = max_width.max(40).min(area.width);
    let height = max_height.max(10).min(area.height);

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    Rect::new(x, y, width, height)
}

/// Render the cleanup confirmation view as a centered modal dialog.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let dialog_area = centered_rect(area, 80);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    let items = collect_selected_items(app);
    let item_count = items.len();
    let total_size: u64 = items.iter().filter_map(|i| i.2).sum();
    let known_count = items.iter().filter(|i| i.2.is_some()).count();

    // Layout inside the dialog: header, items list, summary, action bar
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with border
            Constraint::Min(3),   // Items table (scrollable)
            Constraint::Length(2), // Summary line
            Constraint::Length(1), // Action bar
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0]);
    render_items_list(app, frame, inner_chunks[1], &items);
    render_summary(app, frame, inner_chunks[2], item_count, total_size, known_count);
    render_action_bar(app, frame, inner_chunks[3]);
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
    items: &[(String, String, Option<u64>)],
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
        .map(|(name, path, size)| {
            let name_cell = Cell::from(Span::styled(name.as_str(), app.theme.style_normal()));

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

            Row::new(vec![name_cell, path_cell, size_cell])
        })
        .collect();

    let widths = [
        Constraint::Length(20),  // Name
        Constraint::Min(20),     // Path
        Constraint::Length(12),  // Size
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
) {
    let size_text = format_size(total_size);
    let suffix = if known_count < item_count {
        format!(" ({} of {} items have known sizes)", known_count, item_count)
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

    let summary = Paragraph::new(Line::from(vec![Span::styled(
        summary_text,
        app.theme
            .style_size()
            .add_modifier(Modifier::BOLD),
    )]))
    .block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(summary, area);
}

fn render_action_bar(app: &App, frame: &mut Frame, area: Rect) {
    let action = Paragraph::new(Line::from(vec![Span::styled(
        " y confirm  n/Esc cancel  q quit ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(action, area);
}
