// Module info overlay — centered modal showing module metadata and actions.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, View};
use crate::module::installer;
use crate::module::manifest::{RestoreKind, RiskLevel};
use crate::tui::widgets::centered_rect;

/// Handle key events for the info overlay.
pub fn handle_key(app: &mut App, key: KeyCode, module_idx: usize) {
    if app.info_confirm_remove {
        match key {
            KeyCode::Char('y') => {
                // Remove the module directory and state
                if let Some(manifest_path) = &app.modules[module_idx].manifest_path {
                    if let Some(module_dir) = manifest_path.parent() {
                        let _ = std::fs::remove_dir_all(module_dir);
                    }
                }
                app.modules.remove(module_idx);
                app.info_confirm_remove = false;

                // Reset views that may hold stale module indices
                app.previous_view = View::ModuleList;
                app.set_view(View::ModuleList);

                // Clamp indices to valid range
                let max_idx = app.modules.len().saturating_sub(1);
                app.selected_index = app.selected_index.min(max_idx);
                app.module_list_index = app.module_list_index.min(max_idx);
                if app.browser_module_idx > module_idx {
                    app.browser_module_idx -= 1;
                } else if app.browser_module_idx == module_idx {
                    app.browser_module_idx = 0;
                }
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                app.info_confirm_remove = false;
            }
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Esc | KeyCode::Char('i') => {
            app.set_view(app.previous_view);
            app.selected_index = 0;
        }
        KeyCode::Char('e') => {
            if let Some(manifest_path) = &app.modules[module_idx].manifest_path {
                app.pending_editor = Some(manifest_path.clone());
            }
        }
        KeyCode::Char('o') => {
            if let Some(manifest_path) = &app.modules[module_idx].manifest_path {
                if let Some(module_dir) = manifest_path.parent() {
                    App::open_in_file_manager(module_dir);
                }
            }
        }
        KeyCode::Char('r') => {
            app.info_confirm_remove = true;
        }
        _ => {}
    }
}

/// Render the info overlay as a centered modal on top of the current view.
pub fn render(app: &mut App, frame: &mut Frame, module_idx: usize) {
    if module_idx >= app.modules.len() {
        return;
    }

    let area = frame.area();
    let dialog_area = centered_rect(area, 70);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Layout: header, metadata content, footer
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(3),    // Metadata content
            Constraint::Length(1), // Footer
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0], module_idx);
    render_metadata(app, frame, inner_chunks[1], module_idx);
    render_footer(app, frame, inner_chunks[2]);
}

fn render_header(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!(" Module Info \u{2014} {}", ms.module.name),
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn render_metadata(app: &mut App, frame: &mut Frame, area: Rect, module_idx: usize) {
    let ms = &app.modules[module_idx];
    let m = &ms.module;

    let label_style = Style::default()
        .fg(app.theme.header_fg)
        .add_modifier(Modifier::BOLD);
    let value_style = app.theme.style_normal();

    let platforms_str = m.platforms.join(", ");
    let targets_str = format!("{}", m.targets.len());
    let manifest_str = ms.manifest_path.as_ref().map(|p| p.display().to_string());

    let mut rows: Vec<Row> = vec![
        metadata_row("Name", &m.name, label_style, value_style),
        metadata_row("Id", &m.id, label_style, value_style),
        metadata_row("Version", &m.version, label_style, value_style),
        metadata_row("Author", &m.author, label_style, value_style),
        metadata_row("Description", &m.description, label_style, value_style),
        metadata_row("Platforms", &platforms_str, label_style, value_style),
        metadata_row("Targets", &targets_str, label_style, value_style),
    ];

    // Per-target restore/risk info (only show targets with non-default values)
    let has_target_metadata = m.targets.iter().any(|t| {
        t.restore != RestoreKind::Auto || t.restore_steps.is_some() || t.risk != RiskLevel::Safe
    });
    if has_target_metadata {
        rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));
        for target in &m.targets {
            let has_info = target.restore != RestoreKind::Auto
                || target.restore_steps.is_some()
                || target.risk != RiskLevel::Safe;
            if !has_info {
                continue;
            }
            let label = target
                .description
                .as_deref()
                .unwrap_or_else(|| target.paths.first().map(|s| s.as_str()).unwrap_or("?"));
            let mut parts: Vec<String> = Vec::new();
            if target.restore == RestoreKind::Manual {
                parts.push("manual restore".to_string());
            }
            if target.risk != RiskLevel::Safe {
                parts.push(format!("{} risk", target.risk));
            }
            let badge = parts.join(", ");
            let badge_style = if matches!(target.risk, RiskLevel::Medium | RiskLevel::High) {
                app.theme.style_warning()
            } else {
                value_style
            };
            rows.push(Row::new(vec![
                Span::styled(label, label_style),
                Span::styled(badge, badge_style),
            ]));
            if let Some(ref steps) = target.restore_steps {
                rows.push(Row::new(vec![
                    Span::raw(""),
                    Span::styled(format!("\u{21b3} {}", steps), app.theme.style_description()),
                ]));
            }
        }
    }

    // Source info (for GitHub-installed modules)
    let source_info = ms
        .manifest_path
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(installer::read_source_info);

    let short_commit;
    let installed_str;
    if let Some(ref source) = source_info {
        rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));
        rows.push(metadata_row(
            "Repository",
            &source.repository,
            label_style,
            value_style,
        ));
        if let Some(ref git_ref) = source.git_ref {
            rows.push(metadata_row("Ref", git_ref, label_style, value_style));
        }
        short_commit = if source.commit.len() > 8 {
            &source.commit[..8]
        } else {
            &source.commit
        };
        rows.push(metadata_row(
            "Commit",
            short_commit,
            label_style,
            value_style,
        ));
        installed_str = format_timestamp(source.installed_at);
        rows.push(metadata_row(
            "Installed",
            &installed_str,
            label_style,
            value_style,
        ));
    }

    // Show manifest path
    if let Some(ref path_str) = manifest_str {
        rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));
        rows.push(metadata_row(
            "Path",
            path_str,
            label_style,
            app.theme.style_description(),
        ));
    }

    // Blank line before actions
    rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));

    // Action bar or remove confirmation
    if app.info_confirm_remove {
        rows.push(Row::new(vec![
            Span::styled("Remove module?", app.theme.style_warning()),
            Span::styled(
                "[y]es  [n]o",
                Style::default()
                    .fg(app.theme.warning_fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        let action_style = Style::default()
            .fg(app.theme.size_fg)
            .add_modifier(Modifier::BOLD);
        rows.push(Row::new(vec![
            Span::styled("[e]dit", action_style),
            Span::styled("[o]pen  [r]emove", action_style),
        ]));
    }

    let widths = [
        Constraint::Length(14), // Label column
        Constraint::Min(20),    // Value column
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

/// Build a single metadata row with styled label and value.
fn metadata_row<'a>(
    label: &'a str,
    value: &'a str,
    label_style: Style,
    value_style: Style,
) -> Row<'a> {
    Row::new(vec![
        Span::styled(label, label_style),
        Span::styled(value, value_style),
    ])
}

/// Format a Unix epoch timestamp as a human-readable relative time.
fn format_timestamp(epoch_secs: u64) -> String {
    let installed = UNIX_EPOCH + Duration::from_secs(epoch_secs);
    let elapsed = SystemTime::now()
        .duration_since(installed)
        .unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let m = secs / 60;
        format!("{} minute{} ago", m, if m == 1 { "" } else { "s" })
    } else if secs < 86400 {
        let h = secs / 3600;
        format!("{} hour{} ago", h, if h == 1 { "" } else { "s" })
    } else {
        let d = secs / 86400;
        format!("{} day{} ago", d, if d == 1 { "" } else { "s" })
    }
}

fn render_footer(app: &mut App, frame: &mut Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![Span::styled(
        " Esc or i to close ",
        app.theme.style_normal(),
    )]));
    frame.render_widget(footer, area);
}
