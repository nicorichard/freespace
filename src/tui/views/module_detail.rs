// Module detail view — shows individual items within a module.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{App, ModuleStatus};
use crate::tui::widgets::{checkbox_str, format_size, format_size_or_placeholder, module_icon, CheckState};

/// Compute item indices sorted by size descending.
/// Items with known sizes sort before those still calculating (None).
pub fn sorted_item_indices(app: &App, module_idx: usize) -> Vec<usize> {
    let items = &app.modules[module_idx].items;
    let mut indices: Vec<usize> = (0..items.len()).collect();
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
        let msg = Paragraph::new("Module not found.")
            .style(app.theme.style_error());
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

    render_title_bar(app, frame, chunks[0], module_idx);
    render_items_table(app, frame, chunks[1], module_idx);
    render_status_bar(app, frame, chunks[2]);
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
    let ms = &app.modules[module_idx];

    if ms.items.is_empty() {
        let msg = match &ms.status {
            ModuleStatus::Loading | ModuleStatus::Discovering => "Scanning for items...",
            ModuleStatus::Error(_) => "Could not scan this module.",
            ModuleStatus::Ready => "No items found.",
        };
        let content = Paragraph::new(msg)
            .style(app.theme.style_normal())
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
            let item = &ms.items[item_idx];

            // Selection checkbox
            let check_state = if app.selected_items.contains(&item.path) {
                CheckState::All
            } else {
                CheckState::None
            };
            let checkbox_cell =
                Cell::from(Span::styled(checkbox_str(&check_state), app.theme.style_normal()));

            // Item name
            let name_cell = Cell::from(Span::styled(&*item.name, app.theme.style_normal()));

            // Size cell
            let size_cell = match item.size {
                Some(size) => {
                    Cell::from(Span::styled(format_size(size), app.theme.style_size()))
                }
                None => match &ms.status {
                    ModuleStatus::Loading | ModuleStatus::Discovering => {
                        Cell::from(Span::styled("calculating...", app.theme.style_status_loading()))
                    }
                    _ => {
                        Cell::from(Span::styled("N/A \u{26a0}", app.theme.style_warning()))
                    }
                },
            };

            Row::new(vec![checkbox_cell, name_cell, size_cell])
        })
        .collect();

    let widths = [
        Constraint::Length(5),  // Checkbox
        Constraint::Min(30),   // Name
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
    let status = Paragraph::new(Line::from(vec![Span::styled(
        " \u{2191}/\u{2193} navigate  Space select  a all  n none  Enter/c clean  Esc back  ? help  q quit ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(status, area);
}
