# Task: Keybindings Helper Bar Redesign

## Goal

Redesign the bottom status bar across all views to use a clear `[key] action` format, remove obvious bindings (arrow keys, Enter), and extract a shared `keybinding_bar()` widget for consistency.

## Why

The current status bars are long strings of plain text like `↑/↓ navigate  Space select  a all ...` that are hard to scan. Users can't quickly distinguish keys from descriptions. Arrow keys and Enter are obvious and waste space.

## How

### 1. Create a shared widget

Add a `keybinding_bar()` function in `src/tui/widgets/shared.rs` that accepts a slice of `(&str, &str)` pairs (key, action) and renders them as styled spans:

- Key in brackets with accent color: `[space]`
- Action text in dimmed/muted style: `select`
- Separator between groups: ` │ ` or similar

### 2. Update each view's status bar

**Module List** (`src/tui/views/module_list.rs` `render_status_bar`):
```
 [space] select │ [a]ll │ [n]one │ [/] filter │ [c]lean │ [?] help │ [q]uit
```

**Module Detail** (`src/tui/views/module_detail.rs` `render_status_bar`):
```
 [space] select │ [a]ll │ [n]one │ [o]pen │ [/] filter │ [c]lean │ [esc] back │ [?] help │ [q]uit
```

**Cleanup Confirm** (`src/tui/views/cleanup_confirm.rs` `render_action_bar`):
```
 [t]rash │ [d]elete │ [n] cancel │ [/] filter │ [q]uit
```

### 3. Styling

- Bracket characters `[` `]` in the theme's muted color
- Key letter inside brackets in the theme's accent/highlight color
- Action text in the theme's muted color
- Separator `│` in a dim color

### 4. Remove obvious bindings

Drop from all bars: `↑/↓ navigate`, `←/→ section`, `Enter details/drill`, `Backspace back`.

## UX

Before:
```
 ↑/↓ navigate  ←/→ section  Space select  a all  n none  Enter details  / filter  c clean  ? help  q quit
```

After:
```
 [space] select │ [a]ll │ [n]one │ [/] filter │ [c]lean │ [?] help │ [q]uit
```

## Verification

- [ ] All three views use the shared `keybinding_bar()` widget
- [ ] Obvious bindings (arrows, Enter, Backspace) are removed
- [ ] Key hints are visually distinct from action descriptions
- [ ] Bar fits within 80-column terminals without wrapping
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

Nothing — standalone improvement.

## Blocks

- `version-display.md` — version indicator will be placed in the redesigned status bar
- `install-selection-ux.md` — ratatui-based install screen reuses the keybinding bar widget
