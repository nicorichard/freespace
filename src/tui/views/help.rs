// Help overlay — centered modal listing all keybindings by context.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::tui::widgets::centered_rect;

/// Handle key events for the help overlay.
pub fn handle_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('?') | KeyCode::Esc => {
            app.set_view(app.previous_view);
            app.selected_index = 0;
        }
        _ => {}
    }
}

/// Render the help overlay as a centered modal on top of the current view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let dialog_area = centered_rect(area, 70);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Layout: header, keybinding sections, footer
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(3),    // Keybindings content
            Constraint::Length(1), // Footer
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0]);
    render_keybindings(app, frame, inner_chunks[1]);
    render_footer(app, frame, inner_chunks[2]);
}

fn render_header(app: &mut App, frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![Span::styled(
        " Keyboard Shortcuts",
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn render_keybindings(app: &mut App, frame: &mut Frame, area: Rect) {
    let section_style = Style::default()
        .fg(app.theme.header_fg)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(app.theme.size_fg)
        .add_modifier(Modifier::BOLD);
    let desc_style = app.theme.style_normal();

    let rows: Vec<Row> = vec![
        // Global section
        Row::new(vec![Span::styled("Global", section_style), Span::raw("")]),
        keybinding_row("q", "Quit application", key_style, desc_style),
        keybinding_row("?", "Toggle help overlay", key_style, desc_style),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Navigation section
        Row::new(vec![
            Span::styled("Navigation (all list views)", section_style),
            Span::raw(""),
        ]),
        keybinding_row("j / \u{2193}", "Move down", key_style, desc_style),
        keybinding_row("k / \u{2191}", "Move up", key_style, desc_style),
        keybinding_row("PgDn", "Jump down 20 items", key_style, desc_style),
        keybinding_row("PgUp", "Jump up 20 items", key_style, desc_style),
        keybinding_row("Home / g", "Jump to first item", key_style, desc_style),
        keybinding_row("End / G", "Jump to last item", key_style, desc_style),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Module List section
        Row::new(vec![
            Span::styled("Module List", section_style),
            Span::raw(""),
        ]),
        keybinding_row("Enter", "Open module details", key_style, desc_style),
        keybinding_row("Space", "Toggle module selection", key_style, desc_style),
        keybinding_row("a", "Select all modules", key_style, desc_style),
        keybinding_row("n", "Deselect all modules", key_style, desc_style),
        keybinding_row("i", "Module info", key_style, desc_style),
        keybinding_row("/", "Search list", key_style, desc_style),
        keybinding_row("f", "Filter by risk / restore", key_style, desc_style),
        keybinding_row("c", "Clean selected items", key_style, desc_style),
        keybinding_row(
            "Tab",
            "Switch between module list and all-items view",
            key_style,
            desc_style,
        ),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Module Detail section
        Row::new(vec![
            Span::styled("Module Detail", section_style),
            Span::raw(""),
        ]),
        keybinding_row("Space", "Toggle item selection", key_style, desc_style),
        keybinding_row("a", "Select all items", key_style, desc_style),
        keybinding_row("n", "Deselect all items", key_style, desc_style),
        keybinding_row("Enter", "Drill into directory", key_style, desc_style),
        keybinding_row("o", "Open in file manager", key_style, desc_style),
        keybinding_row("i", "Module info", key_style, desc_style),
        keybinding_row("/", "Search list", key_style, desc_style),
        keybinding_row("f", "Filter by risk / restore", key_style, desc_style),
        keybinding_row("c", "Clean selected items", key_style, desc_style),
        keybinding_row(
            "Backspace / Esc",
            "Back (up one level / module list)",
            key_style,
            desc_style,
        ),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Cleanup section
        Row::new(vec![
            Span::styled("Cleanup Confirm", section_style),
            Span::raw(""),
        ]),
        keybinding_row("Space", "Toggle item check", key_style, desc_style),
        keybinding_row("a", "Toggle all checks", key_style, desc_style),
        keybinding_row("t", "Move to trash", key_style, desc_style),
        keybinding_row("d", "Permanently delete", key_style, desc_style),
        keybinding_row("n / Esc", "Cancel and go back", key_style, desc_style),
    ];

    let widths = [
        Constraint::Length(20), // Key column
        Constraint::Min(20),    // Description column
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal());

    frame.render_widget(table, area);
}

/// Build a single keybinding row with styled key and description.
fn keybinding_row<'a>(
    key: &'a str,
    description: &'a str,
    key_style: Style,
    desc_style: Style,
) -> Row<'a> {
    Row::new(vec![
        Span::styled(key, key_style),
        Span::styled(description, desc_style),
    ])
}

fn render_footer(app: &mut App, frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        " ? or Esc to close ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(footer, area);
}
