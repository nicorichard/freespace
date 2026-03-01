// Help overlay — centered modal listing all keybindings by context.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

/// Compute a centered rectangle that is at most `max_percent` of the terminal area.
fn centered_rect(area: Rect, max_percent: u16) -> Rect {
    let max_width = area.width * max_percent / 100;
    let max_height = area.height * max_percent / 100;

    let width = max_width.max(40).min(area.width);
    let height = max_height.max(10).min(area.height);

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    Rect::new(x, y, width, height)
}

/// Render the help overlay as a centered modal on top of the current view.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let dialog_area = centered_rect(area, 70);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Layout: header, keybinding sections, footer
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(3),   // Keybindings content
            Constraint::Length(1), // Footer
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0]);
    render_keybindings(app, frame, inner_chunks[1]);
    render_footer(app, frame, inner_chunks[2]);
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
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

fn render_keybindings(app: &App, frame: &mut Frame, area: Rect) {
    let section_style = Style::default()
        .fg(app.theme.header_fg)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(app.theme.size_fg)
        .add_modifier(Modifier::BOLD);
    let desc_style = app.theme.style_normal();

    let rows: Vec<Row> = vec![
        // Global section
        Row::new(vec![
            Span::styled("Global", section_style),
            Span::raw(""),
        ]),
        keybinding_row("q", "Quit application", key_style, desc_style),
        keybinding_row("?", "Toggle help overlay", key_style, desc_style),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Module List section
        Row::new(vec![
            Span::styled("Module List", section_style),
            Span::raw(""),
        ]),
        keybinding_row("j / \u{2193}", "Move down", key_style, desc_style),
        keybinding_row("k / \u{2191}", "Move up", key_style, desc_style),
        keybinding_row("Enter", "Open module details", key_style, desc_style),
        keybinding_row("Space", "Toggle module selection", key_style, desc_style),
        keybinding_row("s", "Cycle sort mode", key_style, desc_style),
        keybinding_row("c", "Clean selected items", key_style, desc_style),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Module Detail section
        Row::new(vec![
            Span::styled("Module Detail", section_style),
            Span::raw(""),
        ]),
        keybinding_row("j / \u{2193}", "Move down", key_style, desc_style),
        keybinding_row("k / \u{2191}", "Move up", key_style, desc_style),
        keybinding_row("Space", "Toggle item selection", key_style, desc_style),
        keybinding_row("a", "Select all items", key_style, desc_style),
        keybinding_row("n", "Deselect all items", key_style, desc_style),
        keybinding_row("Enter / c", "Clean selected items", key_style, desc_style),
        keybinding_row("Backspace / Esc", "Back to module list", key_style, desc_style),
        Row::new(vec![Span::raw(""), Span::raw("")]),
        // Cleanup section
        Row::new(vec![
            Span::styled("Cleanup Confirm", section_style),
            Span::raw(""),
        ]),
        keybinding_row("y", "Confirm cleanup", key_style, desc_style),
        keybinding_row("n / Esc", "Cancel and go back", key_style, desc_style),
    ];

    let widths = [
        Constraint::Length(20), // Key column
        Constraint::Min(20),   // Description column
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

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        " ? or Esc to close ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(footer, area);
}
