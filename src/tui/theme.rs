// Color theme and styling definitions.

use ratatui::style::{Color, Modifier, Style};

/// Centralized theme definition for consistent styling across all TUI views.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub header_fg: Color,
    pub header_bg: Color,
    pub size_fg: Color,
    pub error_fg: Color,
    pub warning_fg: Color,
    pub status_loading: Color,
    pub status_ready: Color,
    pub status_error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        // 256-color palette values for broad terminal compatibility
        // (Terminal.app, iTerm2, Alacritty, Kitty, WezTerm)
        Self {
            background: Color::Reset,
            foreground: Color::Indexed(252),       // light gray
            border: Color::Indexed(240),           // mid gray
            selected_bg: Color::Indexed(236),      // dark gray highlight
            selected_fg: Color::Indexed(255),      // bright white
            header_fg: Color::Indexed(75),         // steel blue
            header_bg: Color::Reset,
            size_fg: Color::Indexed(222),          // light gold/yellow
            error_fg: Color::Indexed(196),         // red
            warning_fg: Color::Indexed(214),       // orange
            status_loading: Color::Indexed(75),    // blue (in progress)
            status_ready: Color::Indexed(114),     // green (done)
            status_error: Color::Indexed(196),     // red (error)
        }
    }
}

impl Theme {
    /// Style for normal text.
    pub fn style_normal(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    /// Style for the currently selected/highlighted row.
    pub fn style_selected(&self) -> Style {
        Style::default()
            .fg(self.selected_fg)
            .bg(self.selected_bg)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for header/title text.
    pub fn style_header(&self) -> Style {
        Style::default()
            .fg(self.header_fg)
            .bg(self.header_bg)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for size display text.
    pub fn style_size(&self) -> Style {
        Style::default().fg(self.size_fg)
    }

    /// Style for border lines.
    pub fn style_border(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// Style for error messages and indicators.
    pub fn style_error(&self) -> Style {
        Style::default().fg(self.error_fg).add_modifier(Modifier::BOLD)
    }

    /// Style for warning messages and indicators.
    pub fn style_warning(&self) -> Style {
        Style::default().fg(self.warning_fg)
    }

    /// Style for a module status indicator based on its state.
    pub fn style_status_loading(&self) -> Style {
        Style::default().fg(self.status_loading)
    }

    /// Style for a ready/complete status indicator.
    pub fn style_status_ready(&self) -> Style {
        Style::default().fg(self.status_ready)
    }

    /// Style for an error status indicator.
    pub fn style_status_error(&self) -> Style {
        Style::default().fg(self.status_error)
    }
}
