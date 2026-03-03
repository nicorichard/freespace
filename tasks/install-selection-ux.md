# Task: Ratatui-Based Module Install Selection

## Goal

Replace the `dialoguer::MultiSelect` used during module installation with a ratatui-based multi-select screen that matches the TUI's look and keybindings.

## Why

The current `dialoguer` widget drops into a different interaction model: no `hjkl` navigation, no `[a]ll`/`[n]one` shortcuts, no consistent styling. Switching to a ratatui screen keeps the entire experience cohesive and lets us reuse existing widgets.

## How

### 1. New install selection view

Create `src/tui/views/install_select.rs` — a full-screen ratatui view for selecting which modules to install.

**State:**
- List of candidate modules with name, description, and selected/unselected state
- All modules pre-selected by default (matching current behavior)
- Cursor position for navigation

**Keybindings:**
- `j`/`k` or `↑`/`↓` — navigate
- `Space` — toggle selection
- `a` — select all
- `n` — select none
- `Enter` — confirm and proceed with installation
- `Esc`/`q` — cancel installation
- `/` — filter (if the filter widget is reusable)

### 2. Integrate into install flow

In `src/module/installer.rs`, replace the `dialoguer::MultiSelect` call (~line 268-276) with a call that launches the ratatui selection screen. This requires:

- The install command to initialize a minimal TUI (terminal setup/teardown)
- Passing the candidate module list to the view
- Returning the selected indices back to the installer

### 3. Reuse existing widgets

- Use the keybinding bar widget from `keybindings-helper.md` for the bottom bar
- Use the same list rendering style as `module_list.rs` (theme colors, selection indicators)
- Reuse the filter widget if available

### 4. Remove dialoguer dependency

After migration, remove `dialoguer` from `Cargo.toml` if it's no longer used anywhere.

## UX

```
 Select modules to install
 ─────────────────────────────────────────

 [x] Docker           Clean up Docker images, containers, and volumes
 [x] Node Modules     Remove node_modules directories
 [ ] Xcode            Clean Xcode derived data and archives
 [x] npm Cache        Clear npm cache directory
 [x] Homebrew         Clean Homebrew cache

 ─────────────────────────────────────────
 [space] select │ [a]ll │ [n]one │ [Enter] install │ [esc] cancel
```

## Verification

- [ ] Install flow uses a ratatui screen instead of dialoguer
- [ ] `j`/`k`/`↑`/`↓` navigation works
- [ ] `Space` toggles individual module selection
- [ ] `a` selects all, `n` deselects all
- [ ] `Enter` proceeds with selected modules
- [ ] `Esc` cancels installation
- [ ] Visual style matches the main TUI (theme colors, selection indicators)
- [ ] `dialoguer` dependency is removed from Cargo.toml
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

- `keybindings-helper.md` — reuses the shared keybinding bar widget
- `module-id.md` — modules are identified by id in the selection list

## Blocks

Nothing.
