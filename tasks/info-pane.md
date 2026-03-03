# Task: Info Pane Overlay

## Goal

Add an `[i]nfo` overlay that shows detailed metadata and actions for the currently highlighted module.

## Why

Users need a way to understand what a module does, where it came from, and manage it — without leaving the TUI. Currently there's no way to view module metadata or perform module management actions.

## How

### 1. New View variant

Add `Info(usize)` to the `View` enum in `src/app.rs`:

```rust
pub enum View {
    ModuleList,
    ModuleDetail(usize),
    CleanupConfirm,
    Help,
    Info(usize),           // NEW: module index
}
```

### 2. Keybinding

Bind `i` in `ModuleList` and `ModuleDetail` views to open the info pane for the currently selected module.

### 3. Info pane content

Create `src/tui/views/info.rs` with a centered overlay (similar to the Help view). Display:

**Module metadata:**
- Name (and id, once `module-id.md` is done)
- Version
- Author
- Description
- Platforms
- Number of targets / patterns

**Source information:**
- Source type (bundled, local, git)
- Git URL if applicable
- Install path on disk

**Actions (keybindings within the overlay):**
- `[e]dit` — open module TOML in `$EDITOR`
- `[r]emove` — remove the module (with confirmation)
- `[o]pen` — open module directory in system file browser
- `[Esc]` — close overlay

### 4. Rendering

Use a centered `Rect` overlay (60-70% of screen width, 60-80% of height) with a bordered block. Render metadata as a two-column layout: label in dim, value in normal/accent style.

### 5. Action handlers

- **Edit:** spawn `$EDITOR` with the module's TOML path (pause TUI, resume after editor exits)
- **Remove:** transition to a confirmation prompt, then remove the module directory and reload
- **Open:** use `open` (macOS) / `xdg-open` (Linux) on the module directory

## UX

Press `i` on "Docker" module:

```
┌─ Docker ─────────────────────────────────────────┐
│                                                   │
│  Name        Docker                               │
│  Version     1.0.0                                │
│  Author      nicorichard                          │
│  Description Clean up Docker images and volumes   │
│  Platforms   macos, linux                          │
│  Targets     3 patterns                           │
│                                                   │
│  Source       bundled                              │
│  Path         ~/.config/freespace/modules/docker  │
│                                                   │
│  [e]dit │ [r]emove │ [o]pen                       │
│                                                   │
│  Esc close                                        │
└───────────────────────────────────────────────────┘
```

## Verification

- [ ] `i` opens info overlay from both ModuleList and ModuleDetail
- [ ] All module metadata fields are displayed
- [ ] `[e]dit` opens the module TOML in `$EDITOR`
- [ ] `[r]emove` removes the module after confirmation
- [ ] `[o]pen` opens the module directory in system file browser
- [ ] `Esc` closes the overlay and returns to the previous view
- [ ] Overlay renders correctly at various terminal sizes
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

- `module-id.md` — info pane should display the module `id` field

## Blocks

Nothing.
