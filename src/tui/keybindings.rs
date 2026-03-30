//! Shared hotkey definitions — single source of truth for the bottom bar and help overlay.

use crate::app::App;

/// How a hotkey should appear in the bottom bar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAppearance {
    /// Standard bracket styling.
    Normal,
    /// Bright / highlighted.
    Highlighted,
    /// Muted / dimmed.
    Dimmed,
}

/// A single hotkey entry displayed in both the status-bar and the help panel.
pub struct HotkeyDef {
    /// Key symbol as shown in the UI (e.g. `"␣"`, `"⇥"`, `"esc"`).
    pub key: &'static str,
    /// Short label for the bottom bar (e.g. `"select"`, `"back"`).
    pub label: &'static str,
    /// Longer description for the help overlay.
    pub desc: &'static str,
    /// Show in the bottom status bar. When `false`, only appears in help.
    pub bar: bool,
    /// Optional callback that determines appearance based on app state.
    /// When `None`, renders with `Normal` styling.
    pub style: Option<fn(&App) -> HotkeyAppearance>,
}

// ---------------------------------------------------------------------------
// Style callbacks
// ---------------------------------------------------------------------------

fn clean_appearance(app: &App) -> HotkeyAppearance {
    if app.selected_items.is_empty() {
        HotkeyAppearance::Dimmed
    } else {
        HotkeyAppearance::Highlighted
    }
}

// ---------------------------------------------------------------------------
// Helper macro to reduce per-entry boilerplate
// ---------------------------------------------------------------------------

/// Normal bar hotkey (shown in both bar and help).
const fn hk(key: &'static str, label: &'static str, desc: &'static str) -> HotkeyDef {
    HotkeyDef {
        key,
        label,
        desc,
        bar: true,
        style: None,
    }
}

/// Help-only hotkey (not shown in bottom bar).
const fn hk_help(key: &'static str, label: &'static str, desc: &'static str) -> HotkeyDef {
    HotkeyDef {
        key,
        label,
        desc,
        bar: false,
        style: None,
    }
}

// ---------------------------------------------------------------------------
// Per-view hotkey tables
// ---------------------------------------------------------------------------

pub const MODULE_LIST: &[HotkeyDef] = &[
    hk("␣", "select", "Toggle module selection"),
    hk("a", "all", "Select all modules"),
    hk("n", "none", "Deselect all modules"),
    hk_help("↵", "open", "Open module details"),
    hk("i", "info", "Module info"),
    hk("/", "search", "Search list"),
    hk("f", "filter", "Filter by risk / restore"),
    HotkeyDef {
        key: "c",
        label: "clean",
        desc: "Clean selected items",
        bar: true,
        style: Some(clean_appearance),
    },
    hk("⇥", "flat", "Switch to all-items view"),
    hk_help("U", "update", "Update all outdated modules"),
    hk("?", "help", "Toggle help overlay"),
    hk_help("q", "quit", "Quit application"),
];

pub const MODULE_DETAIL: &[HotkeyDef] = &[
    hk("␣", "select", "Toggle item selection"),
    hk("a", "all", "Select all items"),
    hk("n", "none", "Deselect all items"),
    hk_help("↵", "drill", "Drill into directory"),
    hk("o", "open", "Open in file manager"),
    hk("i", "info", "Module info"),
    hk("/", "search", "Search list"),
    hk("f", "filter", "Filter by risk / restore"),
    HotkeyDef {
        key: "c",
        label: "clean",
        desc: "Clean selected items",
        bar: true,
        style: Some(clean_appearance),
    },
    hk("esc", "back", "Back to module list"),
    hk("?", "help", "Toggle help overlay"),
    hk_help("q", "quit", "Quit application"),
];

pub const FLAT_VIEW: &[HotkeyDef] = &[
    hk("␣", "select", "Toggle item selection"),
    hk("a", "all", "Select all items"),
    hk("n", "none", "Deselect all items"),
    hk("o", "open", "Open in file manager"),
    hk("/", "search", "Search list"),
    hk("f", "filter", "Filter by risk / restore"),
    HotkeyDef {
        key: "c",
        label: "clean",
        desc: "Clean selected items",
        bar: true,
        style: Some(clean_appearance),
    },
    hk("⇥", "grouped", "Switch to module list"),
    hk("?", "help", "Toggle help overlay"),
    hk_help("q", "quit", "Quit application"),
];

pub const FILE_BROWSER: &[HotkeyDef] = &[
    hk("␣", "select", "Toggle item selection"),
    hk("a", "all", "Select all items"),
    hk("n", "none", "Deselect all items"),
    hk("o", "open", "Open in file manager"),
    hk("/", "search", "Search list"),
    hk("f", "filter", "Filter by risk / restore"),
    HotkeyDef {
        key: "c",
        label: "clean",
        desc: "Clean selected items",
        bar: true,
        style: Some(clean_appearance),
    },
    hk("esc", "back", "Back to parent"),
    hk("?", "help", "Toggle help overlay"),
    hk_help("q", "quit", "Quit application"),
];

pub const CLEANUP_CONFIRM: &[HotkeyDef] = &[
    hk("␣", "toggle", "Toggle item check"),
    hk("a", "all", "Toggle all checks"),
    hk("t", "trash", "Move to trash"),
    hk("d", "delete", "Permanently delete"),
    hk("n", "cancel", "Cancel and go back"),
    hk("/", "search", "Search list"),
];

// ---------------------------------------------------------------------------
// Install select
// ---------------------------------------------------------------------------

pub const INSTALL_SELECT: &[HotkeyDef] = &[
    hk("␣", "toggle", "Toggle module"),
    hk("a", "all", "Select all"),
    hk("n", "none", "Deselect all"),
    hk("enter", "confirm", "Confirm selection"),
    hk("esc", "cancel", "Cancel"),
];

// ---------------------------------------------------------------------------
// Overlay bars
// ---------------------------------------------------------------------------

pub const FILTER_MENU: &[HotkeyDef] = &[
    hk("f", "close", "Close filter menu"),
    hk("r", "reset", "Reset filters"),
];

// ---------------------------------------------------------------------------
// Cleanup progress (small context-specific bars)
// ---------------------------------------------------------------------------

pub const CLEANUP_PROGRESS_ACTIVE: &[HotkeyDef] = &[hk("esc", "halt", "Halt cleanup")];

pub const CLEANUP_PROGRESS_HALTED: &[HotkeyDef] = &[
    hk("q", "quit", "Quit application"),
    hk("any", "continue", "Continue cleanup"),
];

// ---------------------------------------------------------------------------
// Navigation keys (shared across all list views, help-only)
// ---------------------------------------------------------------------------

pub const NAVIGATION: &[HotkeyDef] = &[
    hk_help("j / ↓", "", "Move down"),
    hk_help("k / ↑", "", "Move up"),
    hk_help("PgDn", "", "Jump down 20 items"),
    hk_help("PgUp", "", "Jump up 20 items"),
    hk_help("Home / g", "", "Jump to first item"),
    hk_help("End / G", "", "Jump to last item"),
];
