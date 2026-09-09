// Cleanup results — shows what could not be removed, and why.
//
// Failures used to be summarised in a status-bar flash that clears after about
// three seconds. That was survivable when every failure was a safety rule, but
// a handler failure carries its command's stderr, which is the only thing that
// explains what went wrong. This view keeps that text on screen until dismissed.

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::App;
use crate::tui::widgets::render_view_status_bar;

const PAGE_SIZE: usize = 10;

/// Handle key events for the cleanup results view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let count = app
        .cleanup_outcome
        .as_ref()
        .map(|o| o.failures.len())
        .unwrap_or(0);

    match key {
        KeyCode::Char('j') | KeyCode::Down => {
            if count > 0 {
                app.selected_index = (app.selected_index + 1) % count;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if count > 0 {
                app.selected_index = if app.selected_index == 0 {
                    count - 1
                } else {
                    app.selected_index - 1
                };
            }
        }
        KeyCode::PageDown => {
            if count > 0 {
                app.selected_index = (app.selected_index + PAGE_SIZE).min(count - 1);
            }
        }
        KeyCode::PageUp => {
            app.selected_index = app.selected_index.saturating_sub(PAGE_SIZE);
        }
        KeyCode::Home | KeyCode::Char('g') => {
            app.selected_index = 0;
        }
        KeyCode::End | KeyCode::Char('G') => {
            app.selected_index = count.saturating_sub(1);
        }
        KeyCode::Char('?') => {
            app.enter_overlay(crate::app::View::Help);
        }
        KeyCode::Esc | KeyCode::Enter | KeyCode::Backspace => {
            app.dismiss_cleanup_results();
        }
        _ => {}
    }
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(3),    // Failure list
            Constraint::Length(2), // Summary
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    render_header(app, frame, chunks[0]);
    render_failures(app, frame, chunks[1]);
    render_summary(app, frame, chunks[2]);

    let count = app
        .cleanup_outcome
        .as_ref()
        .map(|o| o.failures.len())
        .unwrap_or(0);
    render_view_status_bar(
        frame,
        chunks[3],
        app,
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l)),
        false,
        "",
        false,
        count,
        count,
        crate::tui::keybindings::CLEANUP_RESULTS,
        app.version_hover,
    );
}

fn render_header(app: &mut App, frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![Span::styled(
        " Cleanup Results",
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn render_failures(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(outcome) = &app.cleanup_outcome else {
        return;
    };

    if outcome.failures.is_empty() {
        let msg = Paragraph::new("Everything selected was removed.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(msg, area);
        return;
    }

    // Wrap the reason so a long stderr line stays readable instead of being cut.
    let reason_width = (area.width as usize).saturating_sub(6).max(20);

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected = 0;

    for (i, failure) in outcome.failures.iter().enumerate() {
        if i == app
            .selected_index
            .min(outcome.failures.len().saturating_sub(1))
        {
            visual_selected = rows.len();
        }

        rows.push(Row::new(vec![Cell::from(Span::styled(
            format!(" {}", failure.label),
            app.theme.style_warning(),
        ))]));

        for line in wrap_reason(&failure.reason, reason_width) {
            rows.push(Row::new(vec![Cell::from(Span::styled(
                line,
                app.theme.style_description(),
            ))]));
        }
    }

    let table = Table::new(rows, [Constraint::Percentage(100)])
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal())
        .row_highlight_style(app.theme.style_selected());

    let mut state = TableState::default();
    *state.offset_mut() = app.view_offset;
    state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut state);
    app.view_offset = state.offset();
}

/// Wrap a failure reason onto continuation lines under a `↳` marker.
fn wrap_reason(reason: &str, width: usize) -> Vec<String> {
    let prefix = "   \u{21b3} ";
    let continuation = "     ";
    let mut lines = Vec::new();
    let mut current = String::from(prefix);
    let mut current_len = prefix.chars().count();

    for word in reason.split_whitespace() {
        let word_len = word.chars().count();
        if current_len > prefix.chars().count() && current_len + 1 + word_len > width {
            lines.push(std::mem::take(&mut current));
            current.push_str(continuation);
            current_len = continuation.chars().count();
        } else if current_len > prefix.chars().count() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(word);
        current_len += word_len;
    }

    // `current` still holds the prefix here, so compare lengths rather than
    // trimming — the marker itself is not whitespace.
    if lines.is_empty() && current_len == prefix.chars().count() {
        lines.push(format!("{prefix}(no detail)"));
    } else {
        lines.push(current);
    }
    lines
}

fn render_summary(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(outcome) = &app.cleanup_outcome else {
        return;
    };

    let failed = outcome.failures.len();
    let mut spans = vec![Span::styled(
        format!(
            " {} removed, {} failed",
            outcome.succeeded,
            outcome.failures.len()
        ),
        app.theme.style_size().add_modifier(Modifier::BOLD),
    )];
    if failed > 0 {
        spans.push(Span::styled(
            "  \u{2014} nothing else was touched",
            app.theme.style_description(),
        ));
    }

    let summary = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(summary, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{CleanupFailure, CleanupOutcome};

    fn app_with_failures() -> App {
        let mut app = App::new_for_test(Vec::new());
        app.cleanup_outcome = Some(CleanupOutcome {
            succeeded: 2,
            failures: vec![
                CleanupFailure {
                    label: "iPhone 14".to_string(),
                    reason: "`xcrun simctl delete ABC` exited 164: Invalid device: ABC".to_string(),
                },
                CleanupFailure {
                    label: "cache".to_string(),
                    reason: "blocked by safety rules: /System".to_string(),
                },
            ],
        });
        app
    }

    #[test]
    fn render_does_not_panic() {
        let mut app = app_with_failures();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn render_with_no_failures_does_not_panic() {
        let mut app = App::new_for_test(Vec::new());
        app.cleanup_outcome = Some(CleanupOutcome {
            succeeded: 3,
            failures: Vec::new(),
        });
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn movement_wraps() {
        let mut app = app_with_failures();
        app.selected_index = 0;
        handle_key(&mut app, KeyCode::Char('k'));
        assert_eq!(app.selected_index, 1, "up from first wraps to last");
        handle_key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected_index, 0, "down from last wraps to first");
    }

    /// Dismissing must clear the outcome, or it reappears after the next run.
    #[test]
    fn esc_dismisses_and_clears() {
        let mut app = app_with_failures();
        handle_key(&mut app, KeyCode::Esc);
        assert!(app.cleanup_outcome.is_none());
    }

    #[test]
    fn long_reasons_wrap_instead_of_truncating() {
        let reason = "`xcrun simctl runtime delete UUID` exited 1: \
                      The runtime is currently in use by a booted device";
        let lines = wrap_reason(reason, 40);
        assert!(lines.len() > 1, "expected wrapping, got {lines:?}");
        assert!(lines[0].starts_with("   \u{21b3} "));
        // No word is lost in the wrap.
        let joined: String = lines.join(" ");
        for word in reason.split_whitespace() {
            assert!(joined.contains(word), "lost '{word}'");
        }
    }

    #[test]
    fn empty_reason_still_renders_a_line() {
        assert_eq!(wrap_reason("", 40), vec!["   \u{21b3} (no detail)"]);
    }
}
