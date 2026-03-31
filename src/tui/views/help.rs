// Help overlay — contextual modal listing keybindings for the current view.
// Generated from the shared keybinding tables in `tui::keybindings`.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, View};
use crate::tui::keybindings::{self, HotkeyDef};
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

    frame.render_widget(Clear, dialog_area);

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

/// Return the title and hotkey table for the view that opened help.
fn context_for_view(view: View) -> (&'static str, &'static [HotkeyDef]) {
    match view {
        View::ModuleList => ("Module List", keybindings::MODULE_LIST),
        View::ModuleDetail(_) => ("Module Detail", keybindings::MODULE_DETAIL),
        View::FlatView => ("All Items", keybindings::FLAT_VIEW),
        View::FileBrowser => ("File Browser", keybindings::FILE_BROWSER),
        View::CleanupConfirm => ("Cleanup Confirm", keybindings::CLEANUP_CONFIRM),
        View::CleanupProgress => ("Cleanup", keybindings::CLEANUP_PROGRESS_ACTIVE),
        View::ModuleInstall => ("Install Modules", keybindings::MODULE_INSTALL),
        // Fallback for overlays that shouldn't normally open help
        _ => ("Module List", keybindings::MODULE_LIST),
    }
}

fn render_header(app: &mut App, frame: &mut Frame, area: Rect) {
    let (title, _) = context_for_view(app.previous_view);
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {} — Keyboard Shortcuts", title),
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn push_hotkeys(
    rows: &mut Vec<Row<'static>>,
    defs: &[HotkeyDef],
    key_style: Style,
    desc_style: Style,
) {
    for hk in defs {
        rows.push(Row::new(vec![
            Span::styled(hk.key, key_style),
            Span::styled(hk.desc, desc_style),
        ]));
    }
}

fn render_keybindings(app: &mut App, frame: &mut Frame, area: Rect) {
    let section_style = Style::default()
        .fg(app.theme.header_fg)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(app.theme.size_fg)
        .add_modifier(Modifier::BOLD);
    let desc_style = app.theme.style_normal();

    let mut rows: Vec<Row<'static>> = Vec::new();

    let section = |title: &'static str, rows: &mut Vec<Row<'static>>| {
        rows.push(Row::new(vec![
            Span::styled(title, section_style),
            Span::raw(""),
        ]));
    };

    // Navigation (shown for all list views, not cleanup progress)
    if !matches!(app.previous_view, View::CleanupProgress) {
        section("Navigation", &mut rows);
        push_hotkeys(&mut rows, keybindings::NAVIGATION, key_style, desc_style);
        rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));
    }

    // Context-specific hotkeys
    let (title, defs) = context_for_view(app.previous_view);
    section(title, &mut rows);
    push_hotkeys(&mut rows, defs, key_style, desc_style);

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

fn render_footer(app: &mut App, frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        " ? or Esc to close ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(footer, area);
}
