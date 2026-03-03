// Shared widget utilities used by both module list and module detail views.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
