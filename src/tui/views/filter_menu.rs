// Filter menu overlay — centered popup for structured filtering by risk level and restore kind.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::tui::widgets::keybinding_bar;

/// Render the filter menu as a centered popup overlay.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Fixed-size popup: 28 wide, 14 tall
    let width = 28u16.min(area.width);
    let height = 14u16.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Filter ")
        .borders(Borders::ALL)
        .border_style(app.theme.style_border());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Layout: content area + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let content_area = chunks[0];
    let footer_area = chunks[1];

    // Build content lines
    let risk_labels = ["Safe", "Low", "Medium", "High"];
    let restore_labels = ["Auto", "Manual"];

    let mut lines: Vec<Line> = Vec::new();

    // Risk level section
    lines.push(Line::from(Span::styled(
        " RISK LEVEL",
        app.theme.style_header(),
    )));
    for (i, label) in risk_labels.iter().enumerate() {
        let checked = if app.filter_risk[i] { "x" } else { " " };
        let cursor_idx = i;
        let style = if app.filter_menu_cursor == cursor_idx {
            app.theme.style_selected()
        } else {
            app.theme.style_normal()
        };
        lines.push(Line::from(Span::styled(
            format!("  [{}] {}", checked, label),
            style,
        )));
    }

    // Blank separator
    lines.push(Line::from(""));

    // Restore section
    lines.push(Line::from(Span::styled(
        " RESTORE",
        app.theme.style_header(),
    )));
    for (i, label) in restore_labels.iter().enumerate() {
        let checked = if app.filter_restore[i] { "x" } else { " " };
        let cursor_idx = 4 + i;
        let style = if app.filter_menu_cursor == cursor_idx {
            app.theme.style_selected()
        } else {
            app.theme.style_normal()
        };
        lines.push(Line::from(Span::styled(
            format!("  [{}] {}", checked, label),
            style,
        )));
    }

    frame.render_widget(Paragraph::new(lines), content_area);

    // Footer with keybinding hints
    let footer = keybinding_bar(&[("f", "close"), ("r", "reset")], &app.theme);
    frame.render_widget(Paragraph::new(footer), footer_area);
}
