// Module info overlay — centered modal showing module metadata and actions.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, ModuleUpdateStatus, SiblingUpdatePrompt, View};
use crate::module::installer;
use crate::module::manifest::{RestoreKind, RiskLevel};
use crate::tui::widgets::{centered_rect, format_size};

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

    if let Some(mut prompt) = app.info_confirm_update.take() {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                prompt.selected = prompt.selected.saturating_sub(1);
                app.info_confirm_update = Some(prompt);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                prompt.selected = (prompt.selected + 1).min(2);
                app.info_confirm_update = Some(prompt);
            }
            KeyCode::Enter => match prompt.selected {
                0 => {
                    let mut all = vec![prompt.current_idx];
                    all.extend(prompt.sibling_indices);
                    app.start_update_modules(&all);
                }
                1 => {
                    app.start_module_update(prompt.current_idx);
                }
                _ => { /* cancel */ }
            },
            KeyCode::Esc => { /* cancel */ }
            _ => {
                app.info_confirm_update = Some(prompt);
            }
        }
        return;
    }

    match key {
        KeyCode::Esc | KeyCode::Char('i') => {
            app.leave_overlay();
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
        KeyCode::Char('u') => {
            let has_update = matches!(
                &app.modules[module_idx].update_status,
                Some(
                    ModuleUpdateStatus::UpdateAvailable { .. }
                        | ModuleUpdateStatus::NewerTagAvailable { .. }
                )
            );
            if has_update {
                let siblings = app.outdated_siblings(module_idx);
                if siblings.is_empty() {
                    app.start_module_update(module_idx);
                } else {
                    app.info_confirm_update = Some(SiblingUpdatePrompt {
                        current_idx: module_idx,
                        sibling_indices: siblings,
                        selected: 0,
                    });
                }
            }
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

    // Render update confirmation modal on top if active
    if app.info_confirm_update.is_some() {
        render_update_confirm(app, frame, area);
    }
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

    // Calculate the available width for value text (area minus label column, borders, padding)
    let label_col_width: usize = 14;
    let border_width: usize = 2; // left + right border
    let value_width = (area.width as usize).saturating_sub(label_col_width + border_width + 1);

    let platforms_str = m.platforms.join(", ");
    let targets_str = format!("{}", m.targets.len());
    let manifest_str = ms.manifest_path.as_ref().map(|p| p.display().to_string());
    let cleaned_bytes = app.stats.module_total(&m.id);
    let cleaned_str = format_size(cleaned_bytes);

    let mut rows: Vec<Row> = vec![
        metadata_row("Name", &m.name, label_style, value_style),
        metadata_row("Id", &m.id, label_style, value_style),
        metadata_row("Version", &m.version, label_style, value_style),
        metadata_row("Author", &m.author, label_style, value_style),
        metadata_row_wrapped(
            "Description",
            &m.description,
            label_style,
            value_style,
            value_width,
        ),
        metadata_row("Platforms", &platforms_str, label_style, value_style),
        metadata_row("Targets", &targets_str, label_style, value_style),
    ];

    if cleaned_bytes > 0 {
        rows.push(metadata_row(
            "Cleaned",
            &cleaned_str,
            label_style,
            value_style,
        ));
    }

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
        rows.push(metadata_row_wrapped(
            "Repository",
            &source.repository,
            label_style,
            value_style,
            value_width,
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

        // Update status row
        let update_row = match &ms.update_status {
            None | Some(ModuleUpdateStatus::Checking) => Some(Row::new(vec![
                Span::styled("Update", label_style),
                Span::styled("checking...", app.theme.style_description()),
            ])),
            Some(ModuleUpdateStatus::UpdateAvailable { new_commit }) => {
                let short = if new_commit.len() > 7 {
                    &new_commit[..7]
                } else {
                    new_commit
                };
                Some(Row::new(vec![
                    Span::styled("Update", label_style),
                    Span::styled(
                        format!("available ({}), press [u] to update", short),
                        app.theme.style_warning(),
                    ),
                ]))
            }
            Some(ModuleUpdateStatus::NewerTagAvailable {
                current_tag,
                latest_tag,
            }) => Some(Row::new(vec![
                Span::styled("Update", label_style),
                Span::styled(
                    format!(
                        "{} -> {} available, press [u] to update",
                        current_tag, latest_tag
                    ),
                    app.theme.style_warning(),
                ),
            ])),
            Some(ModuleUpdateStatus::UpToDate) => Some(Row::new(vec![
                Span::styled("Update", label_style),
                Span::styled("up to date", app.theme.style_description()),
            ])),
            Some(ModuleUpdateStatus::Skipped | ModuleUpdateStatus::Failed(_)) => None,
        };
        if let Some(row) = update_row {
            rows.push(row);
        }
    }

    // Show manifest path
    if let Some(ref path_str) = manifest_str {
        rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));
        rows.push(metadata_row_wrapped(
            "Path",
            path_str,
            label_style,
            app.theme.style_description(),
            value_width,
        ));
    }

    // Blank line before actions
    rows.push(Row::new(vec![Span::raw(""), Span::raw("")]));

    // Action bar, remove confirmation, or update confirmation
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
        let has_update = matches!(
            &ms.update_status,
            Some(
                ModuleUpdateStatus::UpdateAvailable { .. }
                    | ModuleUpdateStatus::NewerTagAvailable { .. }
            )
        );
        let right_actions = if has_update {
            "[o]pen  [u]pdate  [r]emove"
        } else {
            "[o]pen  [r]emove"
        };
        rows.push(Row::new(vec![
            Span::styled("[e]dit", action_style),
            Span::styled(right_actions, action_style),
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

/// Build a single metadata row with styled label and value, wrapping the value
/// text if it exceeds `value_width` characters.
fn metadata_row<'a>(
    label: &'a str,
    value: &'a str,
    label_style: Style,
    value_style: Style,
) -> Row<'a> {
    metadata_row_wrapped(label, value, label_style, value_style, 0)
}

/// Build a metadata row, wrapping the value text if `value_width > 0` and the
/// text is longer than that width.
fn metadata_row_wrapped<'a>(
    label: &'a str,
    value: &'a str,
    label_style: Style,
    value_style: Style,
    value_width: usize,
) -> Row<'a> {
    if value_width == 0 || value.len() <= value_width {
        return Row::new(vec![
            Span::styled(label, label_style),
            Span::styled(value, value_style),
        ]);
    }

    // Wrap value into multiple lines
    let mut lines: Vec<Line<'a>> = Vec::new();
    let chars: Vec<char> = value.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + value_width).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        lines.push(Line::from(Span::styled(chunk, value_style)));
        start = end;
    }

    let height = lines.len();
    use ratatui::widgets::Cell;
    Row::new(vec![
        Cell::from(Span::styled(label, label_style)),
        Cell::from(ratatui::text::Text::from(lines)),
    ])
    .height(height as u16)
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

/// Render a confirmation modal for updating sibling modules from the same repo.
fn render_update_confirm(app: &mut App, frame: &mut Frame, area: Rect) {
    let prompt = match &app.info_confirm_update {
        Some(v) => v,
        None => return,
    };

    let sibling_count = prompt.sibling_indices.len();
    let selected = prompt.selected;

    let height: u16 = 9;
    let width = (area.width * 50 / 100).max(44).min(area.width);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, modal_area);

    let bold = Style::default()
        .fg(app.theme.header_fg)
        .add_modifier(Modifier::BOLD);
    let normal = app.theme.style_normal();
    let dim = app.theme.style_description();

    let options = [
        format!("Update all {} modules", sibling_count + 1),
        "Update only this module".to_string(),
        "Cancel".to_string(),
    ];

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "  {} other module{} from the same repository",
                sibling_count,
                if sibling_count == 1 { " is" } else { "s are" }
            ),
            bold,
        )),
        Line::from(Span::styled("  also outdated.", bold)),
        Line::from(""),
    ];

    for (i, label) in options.iter().enumerate() {
        let (marker, style) = if i == selected {
            ("\u{25b8} ", normal) // ▸
        } else {
            ("  ", dim)
        };
        lines.push(Line::from(Span::styled(
            format!("  {} {}", marker, label),
            style,
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(app.theme.style_border())
        .style(app.theme.style_normal());

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal_area);
}
