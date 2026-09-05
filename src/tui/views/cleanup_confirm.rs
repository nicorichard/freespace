// Cleanup confirmation dialog — centered modal showing items to be deleted.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{matches_filter, App};
use crate::core::safety::SafetyLevel;
use crate::module::manifest::{RestoreKind, RiskLevel};
use crate::tui::widgets::{
    cmp_size_desc, format_size, format_size_or_placeholder, render_view_status_bar,
};

/// Number of items to jump when pressing Page Up/Down.
const PAGE_SIZE: usize = 20;

/// Handle key events for the cleanup confirmation view.
pub fn handle_key(app: &mut App, key: KeyCode) {
    let count = filtered_confirm_item_count(app);

    match key {
        // Navigate down
        KeyCode::Char('j') | KeyCode::Down => {
            if count > 0 {
                app.selected_index = (app.selected_index + 1) % count;
            }
        }
        // Navigate up
        KeyCode::Char('k') | KeyCode::Up => {
            if count > 0 {
                app.selected_index = if app.selected_index == 0 {
                    count - 1
                } else {
                    app.selected_index - 1
                };
            }
        }
        // Page Down
        KeyCode::PageDown => {
            if count > 0 {
                app.selected_index = (app.selected_index + PAGE_SIZE).min(count - 1);
            }
        }
        // Page Up
        KeyCode::PageUp => {
            if count > 0 {
                app.selected_index = app.selected_index.saturating_sub(PAGE_SIZE);
            }
        }
        // Home / g: jump to first item
        KeyCode::Home | KeyCode::Char('g') => {
            app.selected_index = 0;
        }
        // End / G: jump to last item
        KeyCode::End | KeyCode::Char('G') => {
            if count > 0 {
                app.selected_index = count - 1;
            }
        }
        // Toggle check on highlighted item
        KeyCode::Char(' ') => {
            let items = collect_selected_items(app);
            let visible: Vec<_> = if app.filter_query.is_empty() {
                items
            } else {
                items
                    .into_iter()
                    .filter(|item| matches_filter(&item.name, &[], &app.filter_query))
                    .collect()
            };
            if let Some(item) = visible.get(app.selected_index) {
                if !app.confirm_checked.remove(&item.path) {
                    app.confirm_checked.insert(item.path.clone());
                }
            }
        }
        // Toggle all checks
        KeyCode::Char('a') => {
            if app.confirm_checked.len() == app.selected_items.len()
                && app.confirm_checked == app.selected_items
            {
                app.confirm_checked.clear();
            } else {
                app.confirm_checked = app.selected_items.clone();
            }
        }
        // Deselect all
        KeyCode::Char('n') => {
            app.confirm_checked.clear();
        }
        // Move to trash (reversible where possible)
        KeyCode::Char('t') => {
            app.request_trash();
        }
        // Permanently delete
        KeyCode::Char('d') => {
            if !app.confirm_checked.is_empty() {
                app.start_cleanup(true);
            }
        }
        // Enter: open trash/delete choice dialog
        KeyCode::Enter => {
            if !app.confirm_checked.is_empty() {
                app.cleanup_action_dialog = true;
                app.cleanup_action_cursor = 0;
            }
        }
        // Esc: clear filter first, then close dialog
        KeyCode::Esc => {
            if !app.filter_query.is_empty() {
                app.clear_filter();
                app.selected_index = 0;
            } else {
                app.clear_filter();
                app.confirm_checked.clear();
                app.set_view(app.previous_view);
                app.selected_index = 0;
            }
        }
        // Toggle help overlay
        KeyCode::Char('?') => {
            app.enter_overlay(crate::app::View::Help);
        }
        // Enter filter mode
        KeyCode::Char('/') => {
            app.filter_active = true;
            app.filter_query.clear();
            app.filter_cursor = 0;
            app.selected_index = 0;
        }
        _ => {}
    }
}

/// Remove paths that are descendants of other paths in the set.
/// E.g. if `/a/b` and `/a/b/c/d` are both present, only `/a/b` is kept.
pub(crate) fn dedup_paths(paths: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    // BTreeSet is already sorted, so parents come before children
    let mut result = HashSet::new();
    let mut deduped = BTreeSet::new();
    for path in paths {
        let dominated = path
            .ancestors()
            .skip(1)
            .any(|a| result.contains(a as &Path));
        if !dominated {
            result.insert(path.clone());
            deduped.insert(path.clone());
        }
    }
    deduped
}

/// Info about a selected item for the cleanup confirmation view.
pub struct ConfirmItem {
    pub name: String,
    pub path: PathBuf,
    pub size: Option<u64>,
    pub safety_level: SafetyLevel,
    pub restore_kind: RestoreKind,
    pub restore_steps: Option<String>,
    pub risk_level: RiskLevel,
    /// The exact command that will run, for handler-backed items. Showing this
    /// verbatim before the user confirms is the whole point: what is displayed
    /// here is what gets executed.
    pub removal_command: Option<String>,
    /// Whether removal can be undone. Handler items cannot be trashed.
    pub reversible: bool,
    /// Real filesystem location for handler items, whose own `path` is a
    /// synthetic identity that would be meaningless on screen.
    pub display_path: Option<PathBuf>,
}

impl ConfirmItem {
    /// The location to show in the item list.
    fn shown_path(&self) -> &std::path::Path {
        self.display_path.as_deref().unwrap_or(&self.path)
    }
}

/// Collect selected items across all modules into a flat list.
/// Also includes items selected during drill-in that aren't direct module items.
/// Deduplicates: removes children whose parent is also selected, and exact duplicates.
pub fn collect_selected_items(app: &App) -> Vec<ConfirmItem> {
    let deduped = dedup_paths(&app.selected_items);
    let mut items: Vec<ConfirmItem> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for ms in &app.modules {
        for item in &ms.items {
            if deduped.contains(&item.path) {
                seen.insert(item.path.clone());
                items.push(ConfirmItem {
                    name: item.name.clone(),
                    path: item.path.clone(),
                    size: item.size,
                    safety_level: item.safety_level,
                    restore_kind: item.restore_kind,
                    restore_steps: item.restore_steps.clone(),
                    risk_level: item.risk_level,
                    removal_command: item.action.removal_command(),
                    reversible: item.reversible(),
                    display_path: item.display_path.clone(),
                });
            }
        }
    }

    // Include drill-in selections not found in module items
    for path in &deduped {
        if !seen.contains(path) {
            let (found_size, found_safety) = app
                .drill
                .lookup_meta(path)
                .unwrap_or((None, SafetyLevel::Safe));
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            items.push(ConfirmItem {
                name,
                path: path.clone(),
                size: found_size,
                safety_level: found_safety,
                restore_kind: RestoreKind::default(),
                restore_steps: None,
                risk_level: RiskLevel::default(),
                // Drill-in selections are always real paths.
                removal_command: None,
                reversible: true,
                display_path: None,
            });
        }
    }

    // Sort by size descending (known sizes first), then by name for ties
    items.sort_by(|a, b| cmp_size_desc(a.size, b.size).then_with(|| a.name.cmp(&b.name)));

    items
}

/// Return the number of confirm-list items visible after filtering.
pub fn filtered_confirm_item_count(app: &App) -> usize {
    let items = collect_selected_items(app);
    if app.filter_query.is_empty() {
        items.len()
    } else {
        items
            .iter()
            .filter(|item| matches_filter(&item.name, &[], &app.filter_query))
            .count()
    }
}

/// Build a mapping from visual row index to item index.
/// Returns `Some(item_idx)` for item rows, `None` for indicator/restore sub-rows.
fn visual_row_to_item_index(items: &[ConfirmItem]) -> Vec<Option<usize>> {
    let mut map = Vec::new();
    for (i, item) in items.iter().enumerate() {
        map.push(Some(i));

        let has_indicators = item.safety_level == SafetyLevel::Warn
            || matches!(item.risk_level, RiskLevel::Medium | RiskLevel::High)
            || item.restore_kind == RestoreKind::Manual;
        if has_indicators {
            map.push(None);
        }
        if item.restore_steps.is_some() {
            map.push(None);
        }
        if item.removal_command.is_some() {
            map.push(None);
        }
    }
    map
}

/// Handle click events for the cleanup confirmation view.
pub fn handle_click(app: &mut App, col: u16, row: u16, area: Rect) -> bool {
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(area);

    let table_area = inner_chunks[1];
    // CleanupConfirm table has LEFT|RIGHT borders only, no top/bottom border
    let content_top = table_area.y;
    let content_height = table_area.height as usize;
    if content_height == 0 || row < content_top {
        return false;
    }
    let clicked_visual_offset = (row - content_top) as usize;
    if clicked_visual_offset >= content_height {
        return false;
    }

    let items = collect_selected_items(app);
    let filtered: Vec<_> = if app.filter_query.is_empty() {
        items
    } else {
        items
            .into_iter()
            .filter(|item| matches_filter(&item.name, &[], &app.filter_query))
            .collect()
    };

    // Build visual-row-to-item-index mapping (None for sub-rows)
    let row_map = visual_row_to_item_index(&filtered);
    let scroll_offset = app.view_offset;
    let clicked_visual_row = scroll_offset + clicked_visual_offset;

    if let Some(&Some(item_idx)) = row_map.get(clicked_visual_row) {
        let on_checkbox = col < table_area.x + 4; // narrower checkbox in confirm view
        app.selected_index = item_idx;
        if on_checkbox {
            app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        }
        return true;
    }
    false
}

/// Render the cleanup confirmation view as a fullscreen view.
pub fn render(app: &mut App, frame: &mut Frame) {
    let dialog_area = frame.area();

    let all_items = collect_selected_items(app);
    let item_count = all_items.len();
    let checked_count = all_items
        .iter()
        .filter(|item| app.confirm_checked.contains(&item.path))
        .count();
    let checked_size: u64 = all_items
        .iter()
        .filter(|item| app.confirm_checked.contains(&item.path))
        .filter_map(|item| item.size)
        .sum();
    let checked_known_count = all_items
        .iter()
        .filter(|item| app.confirm_checked.contains(&item.path) && item.size.is_some())
        .count();
    let warned_count = all_items
        .iter()
        .filter(|item| {
            app.confirm_checked.contains(&item.path) && item.safety_level == SafetyLevel::Warn
        })
        .count();
    let risky_count = all_items
        .iter()
        .filter(|item| {
            app.confirm_checked.contains(&item.path)
                && matches!(item.risk_level, RiskLevel::Medium | RiskLevel::High)
        })
        .count();
    let irreversible_count = all_items
        .iter()
        .filter(|item| app.confirm_checked.contains(&item.path) && !item.reversible)
        .count();

    // Apply filter for display, but keep unfiltered totals for summary
    let filtered_items: Vec<_> = if app.filter_query.is_empty() {
        all_items
    } else {
        all_items
            .into_iter()
            .filter(|item| matches_filter(&item.name, &[], &app.filter_query))
            .collect()
    };

    // Layout inside the dialog: header, items list, summary, action bar
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header with border
            Constraint::Min(3),    // Items table (scrollable)
            Constraint::Length(2), // Summary line
            Constraint::Length(1), // Action bar
        ])
        .split(dialog_area);

    render_header(app, frame, inner_chunks[0]);
    render_items_list(app, frame, inner_chunks[1], &filtered_items);
    render_summary(
        app,
        frame,
        inner_chunks[2],
        checked_count,
        item_count,
        checked_size,
        checked_known_count,
        warned_count,
        risky_count,
        irreversible_count,
    );
    render_action_bar(
        app,
        frame,
        inner_chunks[3],
        filtered_items.len(),
        item_count,
    );
}

fn render_header(app: &mut App, frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(Line::from(vec![Span::styled(
        " Cleanup Preview",
        app.theme.style_header(),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(header, area);
}

fn render_items_list(app: &mut App, frame: &mut Frame, area: Rect, items: &[ConfirmItem]) {
    if items.is_empty() {
        let msg = Paragraph::new("No items selected.")
            .style(app.theme.style_normal())
            .block(
                Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT)
                    .border_style(app.theme.style_border()),
            );
        frame.render_widget(msg, area);
        return;
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut visual_selected: usize = 0;

    for (i, item) in items.iter().enumerate() {
        if i == app.selected_index.min(items.len().saturating_sub(1)) {
            visual_selected = rows.len();
        }

        let checked = app.confirm_checked.contains(&item.path);
        let check_cell = Cell::from(Span::styled(
            if checked { "[x]" } else { "[ ]" },
            app.theme.style_normal(),
        ));

        let is_warned = item.safety_level == SafetyLevel::Warn;
        let is_risky = matches!(item.risk_level, RiskLevel::Medium | RiskLevel::High);
        let is_manual = item.restore_kind == RestoreKind::Manual;
        let name_style = if is_warned || is_risky || !item.reversible {
            app.theme.style_warning()
        } else {
            app.theme.style_normal()
        };

        // Combine name and path into one expanding cell
        let path_str = item.shown_path().display().to_string();
        let max_path_len = (area.width as usize).saturating_sub(19 + item.name.len());
        let path_display = if path_str.len() > max_path_len && max_path_len > 3 {
            format!("...{}", &path_str[path_str.len() - (max_path_len - 3)..])
        } else {
            path_str
        };
        let content_cell = Cell::from(Line::from(vec![
            Span::styled(item.name.as_str(), name_style),
            Span::raw("  "),
            Span::styled(path_display, Style::default().fg(app.theme.border)),
        ]));

        let size_cell = Cell::from(Span::styled(
            format_size_or_placeholder(item.size),
            app.theme.style_size(),
        ));

        rows.push(Row::new(vec![check_cell, content_cell, size_cell]));

        // Add indicator sub-line beneath the item if needed
        let mut parts: Vec<String> = Vec::new();
        if is_warned {
            parts.push("[!]".to_string());
        }
        if is_risky {
            parts.push(format!("[{} risk]", item.risk_level));
        }
        if is_manual {
            parts.push("[manual restore]".to_string());
        }
        if !item.reversible {
            parts.push("[cannot be undone]".to_string());
        }
        if !parts.is_empty() {
            let indicator_text = format!("   \u{2014} {}", parts.join(" "));
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(Span::styled(indicator_text, app.theme.style_warning())),
                Cell::from(""),
            ]));
        }

        // Add restore steps sub-line if present
        if let Some(restore) = &item.restore_steps {
            let restore_text = format!("   \u{21b3} Restore: {}", restore);
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(Span::styled(restore_text, app.theme.style_description())),
                Cell::from(""),
            ]));
        }

        // Add the literal command for handler-backed items. Keep this in step
        // with `visual_row_to_item_index` or click mapping desyncs.
        if let Some(command) = &item.removal_command {
            let command_text = format!("   \u{26a1} runs: {}", command);
            rows.push(Row::new(vec![
                Cell::from(""),
                Cell::from(Span::styled(command_text, app.theme.style_warning())),
                Cell::from(""),
            ]));
        }
    }

    let widths = [
        Constraint::Length(3),  // Check
        Constraint::Min(20),    // Content (name + path, or sub-line text)
        Constraint::Length(12), // Size
    ];

    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(app.theme.style_border()),
        )
        .style(app.theme.style_normal())
        .row_highlight_style(app.theme.style_selected());

    // Scroll the table based on selected_index (mapped to visual row)
    let mut state = TableState::default();
    *state.offset_mut() = app.view_offset;
    state.select(Some(visual_selected));
    frame.render_stateful_widget(table, area, &mut state);
    app.view_offset = state.offset();
}

#[allow(clippy::too_many_arguments)]
fn render_summary(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    checked_count: usize,
    total_count: usize,
    checked_size: u64,
    checked_known_count: usize,
    warned_count: usize,
    risky_count: usize,
    irreversible_count: usize,
) {
    let size_text = format_size(checked_size);
    let suffix = if checked_known_count < checked_count {
        format!(
            " ({} of {} checked items have known sizes)",
            checked_known_count, checked_count
        )
    } else {
        String::new()
    };

    let summary_text = format!(
        " {} of {} item{} \u{2014} {} to reclaim{}",
        checked_count,
        total_count,
        if total_count == 1 { "" } else { "s" },
        size_text,
        suffix,
    );

    let mut spans = vec![Span::styled(
        summary_text,
        app.theme.style_size().add_modifier(Modifier::BOLD),
    )];
    if warned_count > 0 {
        spans.push(Span::styled(
            format!(
                " [!] {} item{} in sensitive location",
                warned_count,
                if warned_count == 1 { "" } else { "s" }
            ),
            app.theme.style_warning(),
        ));
    }
    if risky_count > 0 {
        spans.push(Span::styled(
            format!(
                " {} item{} with elevated risk \u{2014} review before proceeding",
                risky_count,
                if risky_count == 1 { "" } else { "s" }
            ),
            app.theme.style_warning(),
        ));
    }
    if irreversible_count > 0 {
        spans.push(Span::styled(
            format!(
                " [!] {} item{} cannot be undone",
                irreversible_count,
                if irreversible_count == 1 { "" } else { "s" }
            ),
            app.theme.style_error(),
        ));
    }

    let summary = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(app.theme.style_border()),
    );
    frame.render_widget(summary, area);
}

fn render_action_bar(app: &mut App, frame: &mut Frame, area: Rect, shown: usize, total: usize) {
    // Surface the irreversible-items prompt inline, as the update-all prompt in
    // `module_list.rs` does, rather than stacking a modal on the confirm screen.
    let confirm_msg;
    let flash = if app.confirm_irreversible_prompt {
        let count = app.irreversible_checked_count();
        confirm_msg = format!(
            "{} item{} cannot be trashed and will be removed permanently. Include {}? [y]es [n]o",
            count,
            if count == 1 { "" } else { "s" },
            if count == 1 { "it" } else { "them" },
        );
        Some((confirm_msg.as_str(), &crate::app::FlashLevel::Warning))
    } else {
        app.flash_message.as_ref().map(|(m, l)| (m.as_str(), l))
    };

    render_view_status_bar(
        frame,
        area,
        app,
        flash,
        app.filter_active,
        &app.filter_query,
        false, // structured filter not applicable in cleanup confirm
        shown,
        total,
        crate::tui::keybindings::CLEANUP_CONFIRM,
        app.version_hover,
    );
}

/// Render the centered trash/delete choice dialog overlay.
pub fn render_action_dialog(app: &App, frame: &mut Frame) {
    use ratatui::widgets::Clear;

    let area = frame.area();
    let width = 30u16.min(area.width);
    let height = 6u16.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Action ")
        .borders(Borders::ALL)
        .border_style(app.theme.style_border());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let options = [
        ("[t]rash", "Move to trash"),
        ("[d]elete", "Permanently delete"),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (i, (label, _desc)) in options.iter().enumerate() {
        let selected = app.cleanup_action_cursor == i;
        let prefix = if selected { " \u{25b8} " } else { "   " };
        let style = if selected {
            app.theme.style_selected()
        } else {
            app.theme.style_normal()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, label),
            style,
        )));
    }

    let content = Paragraph::new(lines).style(app.theme.style_normal());
    frame.render_widget(content, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Item, ItemType, ModuleState, ModuleStatus, View};
    use crate::module::manifest::{Module, Target};
    use std::path::PathBuf;

    /// A confirm app containing one handler-backed item.
    fn make_handler_confirm_app() -> App {
        let mut app = make_confirm_app();
        let action = crate::core::cleaner::CleanupAction::Handler {
            handler: "xcode.simulator-devices",
            id: "ABC-123".to_string(),
        };
        let path = crate::core::handlers::identity_path("xcode.simulator-devices", "ABC-123");
        app.modules[0].items.push(Item {
            name: "iPhone 14".to_string(),
            path: path.clone(),
            size: Some(500_000_000),
            item_type: ItemType::Directory,
            display_path: Some(PathBuf::from(
                "/Users/x/Library/Developer/CoreSimulator/Devices/ABC-123",
            )),
            risk_level: crate::module::manifest::RiskLevel::Medium,
            restore_steps: Some("Recreate in Xcode".to_string()),
            action,
            ..Default::default()
        });
        app.selected_items.insert(path.clone());
        app.confirm_checked.insert(path);
        app
    }

    /// The confirmation screen must show the literal command, because that
    /// display is the user's only chance to see what will run.
    #[test]
    fn handler_items_carry_their_command_into_the_confirm_list() {
        let app = make_handler_confirm_app();
        let items = collect_selected_items(&app);
        let handler_item = items
            .iter()
            .find(|i| i.name == "iPhone 14")
            .expect("handler item present");

        assert_eq!(
            handler_item.removal_command.as_deref(),
            Some("xcrun simctl delete ABC-123")
        );
        assert!(!handler_item.reversible);
        // The synthetic identity must never be what the user sees.
        assert!(handler_item
            .shown_path()
            .starts_with("/Users/x/Library/Developer/CoreSimulator"));
    }

    /// `visual_row_to_item_index` and `render_items_list` must agree, or clicks
    /// land on the wrong row.
    #[test]
    fn row_mapping_accounts_for_the_command_sub_row() {
        let app = make_handler_confirm_app();
        let items = collect_selected_items(&app);
        let map = visual_row_to_item_index(&items);

        let handler_idx = items.iter().position(|i| i.name == "iPhone 14").unwrap();

        // Each item owns exactly one selectable row; sub-rows map to None so
        // they cannot be selected or clicked independently.
        assert_eq!(map.iter().filter(|e| **e == Some(handler_idx)).count(), 1);
        assert_eq!(map.iter().filter(|e| e.is_some()).count(), items.len());

        // The handler item contributes indicator, restore and command sub-rows
        // on top of its main row.
        let handler_row = map.iter().position(|e| *e == Some(handler_idx)).unwrap();
        let sub_rows = map[handler_row + 1..]
            .iter()
            .take_while(|e| e.is_none())
            .count();
        assert_eq!(sub_rows, 3, "indicators + restore steps + command");
    }

    /// Trashing must never silently promote itself into a permanent delete.
    #[test]
    fn trashing_irreversible_items_asks_first() {
        let mut app = make_handler_confirm_app();
        assert!(app.irreversible_checked_count() > 0);

        app.request_trash();
        assert!(
            app.confirm_irreversible_prompt,
            "should prompt rather than silently deleting permanently"
        );
    }

    /// Declining the prompt unchecks the items that cannot be trashed.
    /// Only the handler item is checked here, so nothing is left to clean and
    /// no background task is spawned.
    #[test]
    fn declining_the_prompt_drops_irreversible_items() {
        let mut app = make_handler_confirm_app();
        let handler_path =
            crate::core::handlers::identity_path("xcode.simulator-devices", "ABC-123");
        app.confirm_checked.clear();
        app.confirm_checked.insert(handler_path.clone());

        app.request_trash();
        assert!(app.confirm_irreversible_prompt);

        app.resolve_irreversible_prompt(false);
        assert!(!app.confirm_irreversible_prompt);
        assert!(
            !app.confirm_checked.contains(&handler_path),
            "declining should uncheck the item that cannot be trashed"
        );
    }

    // Proceeds into `start_cleanup`, which spawns a blocking task.
    #[tokio::test]
    async fn path_only_selections_do_not_prompt() {
        let mut app = make_confirm_app();
        app.dry_run = true;
        assert_eq!(app.irreversible_checked_count(), 0);
        app.request_trash();
        assert!(
            !app.confirm_irreversible_prompt,
            "ordinary path items trash without an extra prompt"
        );
    }

    #[test]
    fn render_with_handler_item_does_not_panic() {
        let mut app = make_handler_confirm_app();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    fn make_confirm_app() -> App {
        let module = Module {
            id: "test".to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            author: "tester".to_string(),
            platforms: vec!["macos".to_string()],
            tags: vec![],
            icon: None,
            icon_color: None,
            targets: vec![Target {
                source: crate::module::manifest::TargetSource::Paths(vec!["~/test".to_string()]),
                description: None,
                restore: crate::module::manifest::RestoreKind::default(),
                restore_steps: None,
                risk: crate::module::manifest::RiskLevel::default(),
                ignore: vec![],
            }],
        };
        let ms = ModuleState {
            module,
            items: vec![
                Item {
                    name: "big".to_string(),
                    path: PathBuf::from("/tmp/big"),
                    size: Some(5_000_000_000),
                    item_type: ItemType::Directory,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
                    restore_kind: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk_level: crate::module::manifest::RiskLevel::default(),
                    ignore_patterns: vec![],
                    ..Default::default()
                },
                Item {
                    name: "small".to_string(),
                    path: PathBuf::from("/tmp/small"),
                    size: Some(1_000),
                    item_type: ItemType::File,
                    target_description: None,
                    safety_level: crate::core::safety::SafetyLevel::Safe,
                    is_shared: false,
                    restore_kind: crate::module::manifest::RestoreKind::default(),
                    restore_steps: None,
                    risk_level: crate::module::manifest::RiskLevel::default(),
                    ignore_patterns: vec![],
                    ..Default::default()
                },
            ],
            total_size: Some(5_000_001_000),
            status: ModuleStatus::Ready,
            origin: crate::module::manager::ModuleOrigin::User,
            manifest_path: None,
            update_status: None,
        };
        let mut app = App::new_for_test(vec![ms]);
        // Select both items
        app.selected_items.insert(PathBuf::from("/tmp/big"));
        app.selected_items.insert(PathBuf::from("/tmp/small"));
        app.confirm_checked = app.selected_items.clone();
        app.current_view = View::CleanupConfirm;
        app.previous_view = View::ModuleList;
        app
    }

    #[test]
    fn collect_selected_items_returns_all() {
        let app = make_confirm_app();
        let items = collect_selected_items(&app);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn collect_selected_items_sorted_by_size_desc() {
        let app = make_confirm_app();
        let items = collect_selected_items(&app);
        // Largest first
        assert_eq!(items[0].name, "big");
        assert_eq!(items[1].name, "small");
    }

    #[test]
    fn filtered_confirm_count_all() {
        let app = make_confirm_app();
        assert_eq!(filtered_confirm_item_count(&app), 2);
    }

    #[test]
    fn filtered_confirm_count_with_filter() {
        let mut app = make_confirm_app();
        app.filter_query = "big".to_string();
        assert_eq!(filtered_confirm_item_count(&app), 1);
    }

    #[test]
    fn render_does_not_panic() {
        let mut app = make_confirm_app();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn toggle_removes_item_from_confirm_checked() {
        let mut app = make_confirm_app();
        assert_eq!(app.confirm_checked.len(), 2);
        // Unchecking the first visible item (sorted by size desc = "big")
        app.confirm_checked.remove(&PathBuf::from("/tmp/big"));
        assert_eq!(app.confirm_checked.len(), 1);
        assert!(!app.confirm_checked.contains(&PathBuf::from("/tmp/big")));
        assert!(app.confirm_checked.contains(&PathBuf::from("/tmp/small")));
    }

    #[test]
    fn render_with_partial_checks_does_not_panic() {
        let mut app = make_confirm_app();
        // Uncheck one item
        app.confirm_checked.remove(&PathBuf::from("/tmp/small"));
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(&mut app, frame)).unwrap();
    }

    #[test]
    fn dedup_paths_removes_children() {
        let mut paths = std::collections::BTreeSet::new();
        paths.insert(PathBuf::from("/a/b"));
        paths.insert(PathBuf::from("/a/b/c"));
        paths.insert(PathBuf::from("/a/b/c/d"));
        let result = dedup_paths(&paths);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&PathBuf::from("/a/b")));
    }

    #[test]
    fn dedup_paths_keeps_disjoint() {
        let mut paths = std::collections::BTreeSet::new();
        paths.insert(PathBuf::from("/a/b"));
        paths.insert(PathBuf::from("/c/d"));
        let result = dedup_paths(&paths);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&PathBuf::from("/a/b")));
        assert!(result.contains(&PathBuf::from("/c/d")));
    }

    #[test]
    fn dedup_paths_single_item() {
        let mut paths = std::collections::BTreeSet::new();
        paths.insert(PathBuf::from("/a/b"));
        let result = dedup_paths(&paths);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn dedup_paths_empty() {
        let paths = std::collections::BTreeSet::new();
        let result = dedup_paths(&paths);
        assert!(result.is_empty());
    }
}
