// Shared widget utilities used by both module list and module detail views.

use std::cmp::Ordering;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Spinner characters (Braille dots) that cycle during scanning/loading.
pub const SPINNER_CHARS: &[char] = &[
    '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}', '\u{2834}', '\u{2826}', '\u{2827}',
    '\u{2807}', '\u{280f}',
];

/// Compare two `Option<u64>` sizes for descending sort order.
///
/// - `Some` values sort descending (largest first).
/// - `None` sorts before `Some` (unknown/loading items appear first).
/// - Two `None` values compare as `Equal`.
pub fn cmp_size_desc(a: Option<u64>, b: Option<u64>) -> Ordering {
    match (b, a) {
        (Some(sb), Some(sa)) => sb.cmp(&sa),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Build a styled keybinding bar from a slice of (key, action) pairs.
///
/// Renders as: `[key] action │ [key] action │ ...`
/// - Brackets `[` `]` in the theme's muted/border color
/// - Key text inside brackets in the theme's accent (size_fg) color
/// - Action text in the theme's muted/border color
/// - Separator `│` in dim border color
pub fn keybinding_bar<'a>(bindings: &[(&'a str, &'a str)], theme: &Theme) -> Line<'a> {
    let bracket_style = theme.style_border();
    let key_style = theme.style_size();
    let action_style = theme.style_border();
    let sep_style = theme.style_border();

    let mut spans: Vec<Span<'a>> = Vec::new();
    spans.push(Span::raw(" "));

    for (i, (key, action)) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" \u{2502} ", sep_style));
        }
        spans.push(Span::styled("[", bracket_style));
        spans.push(Span::styled(*key, key_style));
        spans.push(Span::styled("] ", bracket_style));
        spans.push(Span::styled(*action, action_style));
    }

    Line::from(spans)
}

/// Render a status bar line with a right-aligned version string.
///
/// Splits `area` into a left region (for `left` content) and a right region
/// showing `vX.Y.Z` in dim style.
pub fn render_status_line(frame: &mut Frame, area: Rect, left: Line<'_>, theme: &Theme) {
    let version = format!("v{} ", env!("CARGO_PKG_VERSION"));
    let version_width = version.len() as u16;

    let chunks =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(version_width)]).split(area);

    frame.render_widget(Paragraph::new(left), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(version, theme.style_border()))),
        chunks[1],
    );
}

/// Get an emoji icon for a module based on its name.
pub fn module_icon(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("xcode") {
        "\u{1f528}" // 🔨
    } else if lower.contains("npm") || lower.contains("yarn") || lower.contains("pnpm") {
        "\u{1f4e6}" // 📦
    } else if lower.contains("homebrew") || lower.contains("brew") {
        "\u{1f37a}" // 🍺
    } else if lower.contains("docker") {
        "\u{1f433}" // 🐳
    } else if lower.contains("cache") {
        "\u{1f5c2}\u{fe0f}" // 🗂️
    } else {
        "\u{1f4c1}" // 📁
    }
}

/// Selection state for a checkbox (module or item level).
pub enum CheckState {
    /// Nothing selected.
    None,
    /// All items selected.
    All,
    /// Some but not all items selected.
    Partial,
}

/// Return the checkbox string for a given check state.
pub fn checkbox_str(state: &CheckState) -> &'static str {
    match state {
        CheckState::None => "[ ]",
        CheckState::All => "[x]",
        CheckState::Partial => "[~]",
    }
}

/// Build a flash message line with appropriate styling based on the flash level.
pub fn flash_line<'a>(message: &'a str, level: &crate::app::FlashLevel, theme: &Theme) -> Line<'a> {
    let style = match level {
        crate::app::FlashLevel::Info => theme.style_size(),
        crate::app::FlashLevel::Warning => theme.style_warning(),
        crate::app::FlashLevel::Error => theme.style_error(),
    };
    Line::from(Span::styled(format!(" {}", message), style))
}

/// Render a standard status bar with flash message, filter input, or keybinding bar.
///
/// This encapsulates the pattern shared by all list views:
/// 1. If a flash message is active, show it
/// 2. If filter input is active, show the filter cursor
/// 3. If a filter query is set, show it with match counts
/// 4. Otherwise show the keybinding bar
#[allow(clippy::too_many_arguments)]
pub fn render_view_status_bar(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    flash: Option<(&str, &crate::app::FlashLevel)>,
    filter_active: bool,
    filter_query: &str,
    shown: usize,
    total: usize,
    bindings: &[(&str, &str)],
) {
    let line = if let Some((msg, level)) = flash {
        flash_line(msg, level, theme)
    } else if filter_active {
        Line::from(vec![
            Span::styled(" / ", theme.style_size()),
            Span::styled(filter_query.to_string(), theme.style_normal()),
            Span::styled("\u{2588}", theme.style_size()),
        ])
    } else if !filter_query.is_empty() {
        Line::from(vec![
            Span::styled(
                format!(" filter: \"{}\" ({}/{})  ", filter_query, shown, total),
                theme.style_size(),
            ),
            Span::styled("/ filter  Esc clear", theme.style_normal()),
        ])
    } else {
        keybinding_bar(bindings, theme)
    };
    render_status_line(frame, area, line, theme);
}

/// Check if a click column is in the checkbox zone of a table area.
/// The checkbox column is the first column, within the table's left border
/// and highlight symbol area.
pub fn is_checkbox_click(col: u16, table_area: Rect) -> bool {
    // border (1) + highlight symbol (2) + checkbox column (5) = 8 chars from table left
    col < table_area.x + 8
}

/// Compute a centered rectangle that is at most `max_percent` of the terminal area.
pub fn centered_rect(area: Rect, max_percent: u16) -> Rect {
    let max_width = area.width * max_percent / 100;
    let max_height = area.height * max_percent / 100;

    let width = max_width.max(40).min(area.width);
    let height = max_height.max(10).min(area.height);

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    Rect::new(x, y, width, height)
}

/// Normalize Emacs/terminal-style Ctrl keybindings to standard arrow keys.
///
/// Ctrl+N -> Down, Ctrl+P -> Up, Ctrl+F -> Right, Ctrl+B -> Left.
/// All other keys pass through unchanged.
pub fn normalize_emacs_key(code: KeyCode, modifiers: KeyModifiers) -> KeyCode {
    if modifiers.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Char('n') => KeyCode::Down,
            KeyCode::Char('p') => KeyCode::Up,
            KeyCode::Char('f') => KeyCode::Right,
            KeyCode::Char('b') => KeyCode::Left,
            _ => code,
        }
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ctrl_n_to_down() {
        assert_eq!(
            normalize_emacs_key(KeyCode::Char('n'), KeyModifiers::CONTROL),
            KeyCode::Down
        );
    }

    #[test]
    fn normalize_ctrl_p_to_up() {
        assert_eq!(
            normalize_emacs_key(KeyCode::Char('p'), KeyModifiers::CONTROL),
            KeyCode::Up
        );
    }

    #[test]
    fn normalize_ctrl_f_to_right() {
        assert_eq!(
            normalize_emacs_key(KeyCode::Char('f'), KeyModifiers::CONTROL),
            KeyCode::Right
        );
    }

    #[test]
    fn normalize_ctrl_b_to_left() {
        assert_eq!(
            normalize_emacs_key(KeyCode::Char('b'), KeyModifiers::CONTROL),
            KeyCode::Left
        );
    }

    #[test]
    fn normalize_passthrough_no_modifier() {
        assert_eq!(
            normalize_emacs_key(KeyCode::Char('n'), KeyModifiers::NONE),
            KeyCode::Char('n')
        );
    }

    #[test]
    fn module_icon_xcode() {
        assert_eq!(module_icon("Xcode Derived Data"), "\u{1f528}");
    }

    #[test]
    fn module_icon_npm() {
        assert_eq!(module_icon("npm-cache"), "\u{1f4e6}");
    }

    #[test]
    fn module_icon_docker() {
        assert_eq!(module_icon("Docker"), "\u{1f433}");
    }

    #[test]
    fn module_icon_homebrew() {
        assert_eq!(module_icon("Homebrew"), "\u{1f37a}");
    }

    #[test]
    fn module_icon_cache_generic() {
        assert_eq!(module_icon("pip-cache"), "\u{1f5c2}\u{fe0f}");
    }

    #[test]
    fn module_icon_unknown() {
        assert_eq!(module_icon("something-random"), "\u{1f4c1}");
    }

    #[test]
    fn checkbox_none() {
        assert_eq!(checkbox_str(&CheckState::None), "[ ]");
    }

    #[test]
    fn checkbox_all() {
        assert_eq!(checkbox_str(&CheckState::All), "[x]");
    }

    #[test]
    fn checkbox_partial() {
        assert_eq!(checkbox_str(&CheckState::Partial), "[~]");
    }

    #[test]
    fn cmp_size_desc_both_some() {
        assert_eq!(cmp_size_desc(Some(100), Some(200)), Ordering::Greater);
        assert_eq!(cmp_size_desc(Some(200), Some(100)), Ordering::Less);
        assert_eq!(cmp_size_desc(Some(100), Some(100)), Ordering::Equal);
    }

    #[test]
    fn cmp_size_desc_none_before_some() {
        assert_eq!(cmp_size_desc(None, Some(100)), Ordering::Less);
        assert_eq!(cmp_size_desc(Some(100), None), Ordering::Greater);
    }

    #[test]
    fn cmp_size_desc_both_none() {
        assert_eq!(cmp_size_desc(None, None), Ordering::Equal);
    }
}
