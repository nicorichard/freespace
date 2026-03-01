// Shared widget utilities used by both module list and module detail views.

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
