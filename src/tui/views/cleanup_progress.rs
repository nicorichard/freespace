// Cleanup-in-progress view — shows progress and handles halt confirmation.

use std::sync::atomic::Ordering;

use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::tui::widgets::{keybinding_bar, render_status_line, SPINNER_CHARS};

/// Handle key events for the cleanup-in-progress view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let halted = app.cleanup_progress.as_ref().is_some_and(|p| p.halted);

    if halted {
        // Already halted: q quits, anything else goes back to previous view
        match key {
            KeyCode::Char('q') => {
                app.should_quit = true;
            }
            _ => {
                app.finish_cleanup();
            }
        }
    } else {
        // Cleanup in progress: q/Ctrl+C/Esc halts
        if matches!(key, KeyCode::Char('q') | KeyCode::Esc) {
            if let Some(cancel) = &app.cleanup_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            if let Some(progress) = &mut app.cleanup_progress {
                progress.halted = true;
            }
        }
    }
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),   // Top spacer
            Constraint::Length(3), // Main message
            Constraint::Length(1), // Current file
            Constraint::Fill(1),   // Bottom spacer
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    let progress = match &app.cleanup_progress {
        Some(p) => p,
        None => return,
    };

    let action = if progress.permanent {
        "Deleting"
    } else {
        "Trashing"
    };

    if progress.halted {
        // Halted confirmation
        let past = if progress.permanent {
            "deleted"
        } else {
            "trashed"
        };
        let msg = format!(
            "Cleanup interrupted: {} of {} items {}.",
            progress.done, progress.total, past
        );
        let para = Paragraph::new(Line::from(Span::styled(msg, app.theme.style_warning())))
            .alignment(Alignment::Center);
        frame.render_widget(para, chunks[1]);

        let hint = Paragraph::new(Line::from(Span::styled(
            "Press q to quit, any other key to continue.",
            app.theme.style_normal(),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, chunks[2]);

        let bar = keybinding_bar(&[("q", "quit"), ("any", "continue")], &app.theme);
        render_status_line(frame, chunks[4], bar, &app.theme);
    } else {
        // In-progress
        let spinner = SPINNER_CHARS[app.tick_count % SPINNER_CHARS.len()];
        let msg = format!(
            "{} {} {}/{} items...",
            spinner, action, progress.done, progress.total
        );
        let para = Paragraph::new(Line::from(Span::styled(msg, app.theme.style_header())))
            .alignment(Alignment::Center);
        frame.render_widget(para, chunks[1]);

        if let Some(path) = &progress.current_path {
            let file_line = Paragraph::new(Line::from(Span::styled(
                path.as_str(),
                app.theme.style_border(),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(file_line, chunks[2]);
        }

        let bar = keybinding_bar(&[("esc", "halt")], &app.theme);
        render_status_line(frame, chunks[4], bar, &app.theme);
    }
}
