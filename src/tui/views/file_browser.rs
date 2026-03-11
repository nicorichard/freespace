// File browser view — standalone directory exploration (drill-in).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{matches_filter, App, ItemType};
use crate::tui::widgets::{
    checkbox_str, flash_line, format_size, keybinding_bar, module_icon, render_status_line,
    CheckState,
};

/// Spinner characters that cycle during loading.
const SPINNER_CHARS: &[char] = &[
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];

/// Compute item indices sorted by size descending for the current drill level.
pub fn sorted_item_indices(app: &App) -> Vec<usize> {
    let items = app.drill.current_items();
    let items = match items {
        Some(items) => items,
        None => return Vec::new(),
    };
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

/// Render the file browser view.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let module_idx = app.browser_module_idx;

    if module_idx >= app.modules.len() {
        let msg = Paragraph::new("Module not found.").style(app.theme.style_error());
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar (breadcrumb)
            Constraint::Min(1),    // Content
            Constraint::Length(1), // Path bar
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_title_bar(app, frame, chunks[0], module_idx);
    render_items_table(app, frame, chunks[1]);
    render_path_bar(app, frame, chunks[2]);
    render_status_bar(app, frame, chunks[3], module_idx);
}

fn render_title_bar(app: &App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let icon = module_icon(&ms.module.name);

    // Breadcrumb: module > dir1 > dir2
    let mut parts = vec![ms.module.name.clone()];
    parts.extend(app.drill.breadcrumb_parts());
    let title_text = format!(" {} {} ", icon, parts.join(" > "));

    let lines = vec![Line::from(vec![Span::styled(
        title_text,
        app.theme.style_header(),
    )])];

    let title = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(title, area);
}

fn render_items_table(app: &App, frame: &mut Frame, area: Rect) {
    let items = match app.drill.current_items() {
        Some(items) => items,
        None => {
            let content = Paragraph::new("No items.")
                .style(app.theme.style_normal())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(app.theme.style_border()),
                );
            frame.render_widget(content, area);
            return;
        }
    };

    if items.is_empty() {
        let content = Paragraph::new("Empty directory.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(content, area);
        return;
    }

    // Show loading indicator until all sizes are known
    if items.iter().any(|item| item.size.is_none()) {
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

    let display_order = sorted_item_indices(app);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    for (pos, &item_idx) in display_order.iter().enumerate() {
        if pos == app.selected_index {
            visual_selected = rows.len();
        }

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
        let name_cell = Cell::from(Span::styled(display_name, app.theme.style_normal()));

        let size_cell = match item.size {
            Some(size) => Cell::from(Span::styled(format_size(size), app.theme.style_size())),
            None => Cell::from(Span::styled(
                "calculating...",
                app.theme.style_status_loading(),
            )),
        };

        rows.push(Row::new(vec![checkbox_cell, name_cell, size_cell]));
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

fn render_path_bar(app: &App, frame: &mut Frame, area: Rect) {
    let items = app.drill.current_items().unwrap_or(&[]);
    let display_order = sorted_item_indices(app);

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
        Line::from(vec![
            Span::styled(" / ", app.theme.style_size()),
            Span::styled(&app.filter_query, app.theme.style_normal()),
            Span::styled("\u{2588}", app.theme.style_size()),
        ])
    } else if !app.filter_query.is_empty() {
        let items = app.drill.current_items().unwrap_or(&[]);
        let sorted = sorted_item_indices(app);
        let total = items.len();
        let shown = sorted.len();
        Line::from(vec![
            Span::styled(
                format!(" filter: \"{}\" ({}/{})  ", app.filter_query, shown, total),
                app.theme.style_size(),
            ),
            Span::styled("/ filter  Esc clear", app.theme.style_normal()),
        ])
    } else {
        let _ = module_idx;
        let bindings: Vec<(&str, &str)> = vec![
            ("space", "select"),
            ("a", "all"),
            ("n", "none"),
            ("o", "open"),
            ("/", "filter"),
            ("c", "clean"),
            ("esc", "back"),
            ("?", "help"),
            ("q", "quit"),
        ];
        keybinding_bar(&bindings, &app.theme)
    };
    render_status_line(frame, area, line, &app.theme);
}
