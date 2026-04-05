# Freespace UI Design Guide

This document defines the design principles, patterns, and conventions for the
freespace TUI. All new views and UI changes **must** follow these rules.

---

## Core Principles

1. **Keyboard-first.** Every interaction is reachable via keyboard. Mouse support
   supplements but never replaces keyboard access.
2. **Consistent movement.** All scrollable/navigable surfaces accept the same
   three input families: arrow keys, vim keys, and emacs keys.
3. **Discoverable.** Every screen shows a `[?] help` hint in the status bar so
   users can always find available actions. (Dialogs are exempt — see below.)
4. **Minimal chrome.** Use color and whitespace to create hierarchy — avoid
   heavy borders and decorative elements.

---

## Layout Structure

Every screen follows this three-region layout:

```
┌─────────────────────────────────────────┐
│  Header — title, context, status        │
├─────────────────────────────────────────┤
│                                         │
│  Body — scrollable content area         │
│                                         │
├─────────────────────────────────────────┤
│  Status bar — hotkeys (left) │ version  │
└─────────────────────────────────────────┘
```

- **Header**: 1–3 rows. Title in `style_header()` (steel blue, bold). May include
  a subtitle/description in `style_description()`.
- **Body**: The main interactive area. Always a `ratatui::Table` with `TableState`
  for row highlighting.
- **Status bar**: Single row. Left side shows context-sensitive hotkey hints,
  right side shows the version string in dim style.

---

## Navigation & Movement

### Universal movement keys

All scrollable surfaces **must** support all three families:

| Action       | Arrow    | Vim  | Emacs      |
|------------- |--------- |----- |----------- |
| Move down    | `↓`      | `j`  | `Ctrl+N`   |
| Move up      | `↑`      | `k`  | `Ctrl+P`   |
| Move right   | `→`      | `l`  | `Ctrl+F`   |
| Move left    | `←`      | `h`  | `Ctrl+B`   |
| Page down    | `PgDn`   |      |            |
| Page up      | `PgUp`   |      |            |
| First item   | `Home`   | `g`  |            |
| Last item    | `End`    | `G`  |            |

Emacs keys are normalized to their arrow equivalents in `shared.rs` so view
handlers only need to match arrow `KeyCode` variants.

Movement **wraps**: pressing down on the last item moves to the first, and
pressing up on the first item moves to the last.

### Mouse support

Scrollable surfaces should support mouse scroll events (`ScrollUp`, `ScrollDown`)
for vertical scrolling wherever keyboard up/down is supported.

### View transitions

- Always use `self.set_view(view)` — never assign `self.current_view` directly.
  `set_view()` resets `view_offset` to prevent stale scroll positions.
- `Enter` drills in (open module, open directory).
- `Esc` or `Backspace` goes back (close overlay, pop drill level, return to
  parent view).
- Overlay transitions (Help, Info) **must** use `app.enter_overlay(view)` and
  `app.leave_overlay()` — these save and restore `selected_index` and
  `view_offset` so the user never loses their scroll position.
- `previous_view` tracks what view to return to from overlays (Help, Info).
- `browser_origin` tracks which view initiated a FileBrowser drill-in.

---

## Selection

### Single selection (cursor)

- The current row is highlighted with `style_selected()` (bright white + bold
  on dark gray) and a `▶` highlight symbol at the row start.
- Cursor position is tracked via `selected_index` per view.

### Multi-selection (checkboxes)

All list views that support acting on items use a checkbox column:

| State   | Display | Meaning                     |
|-------- |-------- |---------------------------- |
| None    | `[ ]`   | No items selected           |
| All     | `[x]`   | All items selected          |
| Partial | `[~]`   | Some child items selected   |

**Required hotkeys for any multi-selectable interface:**

| Key         | Action                                      |
|------------ |-------------------------------------------- |
| `Space`     | Toggle selection on current item             |
| `a`         | Select **a**ll visible items                 |
| `n`         | Select **n**one (deselect all visible items) |

- `a` and `n` respect the active filter — they only affect items matching the
  current query.
- Toggling a parent (module) selects/deselects all its children.
- Toggling a child updates the parent to `Partial` or `All` accordingly.
- Selections persist across view changes and filter changes.

---

## Search & Filtering

Any large list **must** support `/` search:

- **`/`** enters filter mode — a text input appears and the list filters in
  real-time as the user types.
- **`Esc`** in filter mode cancels and clears the query.
- **`Enter`** in filter mode accepts the query and exits filter mode (query
  stays active).
- Matching is **case-insensitive substring** search.
- `#hashtag` prefix searches tags only; plain text searches both name and tags.
- Arrow keys pass through during filter mode so the user can still navigate the
  filtered list while typing.
- The filter applies per-view:
  - **ModuleList**: filters by module name + tags
  - **ModuleDetail**: filters by item name
  - **FlatView**: filters by item name or module name
  - **FileBrowser**: filters by entry name
  - **CleanupConfirm**: filters by item name

---

## Screens vs Dialogs

The UI has two categories of view:

- **Screens** are full views the user spends time in (ModuleList, ModuleDetail,
  FlatView, FileBrowser, CleanupConfirm, ModuleInstall). They have the full
  header/body/status-bar layout and **must** support `[?] help`, movement keys,
  `/` search, multi-select, and mouse scroll.
- **Dialogs** are small, transient call-to-action surfaces (Help overlay, Info
  overlay, FilterMenu, CleanupProgress, inline confirmations like `[y]es [n]o`).
  They do **not** need `[?] help`, search, or multi-select — they should be
  minimal and fast to dismiss. They only need the keys directly relevant to the
  action at hand.

---

## Dialogs & Overlays

### Help overlay (`?`)

- Accessible from every **screen** via `?`.
- Rendered as a centered modal (70% terminal width/height) with a `Clear`
  widget to erase the background.
- Shows all keybindings for the current context, organized by section.
- Dismissed with `?` or `Esc`.

### Info overlay (`i`)

- Shows module metadata (name, version, author, description, targets, etc.).
- Actions within the overlay: `e` edit manifest, `o` open directory, `r` remove.
- Sub-confirmations (e.g., remove) use inline `[y]es [n]o` prompts rather than
  nested modals.

### CleanupProgress dialog

- Shows a spinner, item counter, and current file path. Not scrollable.
- `Ctrl+C` halts; halted state offers `q` to quit or any key to continue.
- No `[?] help` needed — the available actions are self-evident.

### Confirmation dialogs

- **Favour movement over hotkeys.** If a dialog presents a list of items
  (e.g., CleanupConfirm), the list is navigable with standard movement keys
  and supports multi-selection. Hotkeys are reserved for final actions
  (`t` trash, `d` delete, `n`/`Esc` cancel).
- Keep confirmations minimal — show what will happen, let the user act or cancel.

---

## Status Bar & Hotkey Hints

The status bar at the bottom of every screen uses this format:

```
[key] action │ [key] action │ [key] action          v0.0.6
```

- Brackets `[ ]` rendered in `style_border()` (mid gray).
- Key character rendered in `style_size()` (gold/yellow — the accent color).
- Action label rendered in `style_border()`.
- Separator `│` (U+2502) in `style_border()`.
- Version string right-aligned in `style_border()`.

### Dynamic highlighting

Hotkey definitions support a `style` callback for conditional appearance:

| Appearance    | Meaning                           | Visual                           |
|-------------- |---------------------------------- |--------------------------------- |
| `Normal`      | Default                           | Standard status bar style        |
| `Highlighted` | Action is ready / relevant        | `style_clean_ready()` (lime pill)|
| `Dimmed`      | Action is unavailable             | `style_disabled()` (muted gray)  |

Example: the `[c] clean` hotkey appears dimmed when nothing is selected, and
highlighted (lime green pill) when items are selected.

### Flash messages

Temporary messages displayed in the status bar area:

- **Info**: `style_size()` (gold)
- **Warning**: `style_warning()` (orange)
- **Error**: `style_error()` (red, bold)
- Auto-clear after ~3 seconds (12 ticks at 250ms).

---

## Theming & Color

### Palette

Uses **256-color indexed** values for broad terminal compatibility (Terminal.app,
iTerm2, Alacritty, Kitty, WezTerm). No true-color (24-bit) dependencies.

| Role              | Color Index | Appearance       |
|------------------ |------------ |----------------- |
| Background        | Reset       | Terminal default  |
| Foreground        | 252         | Light gray        |
| Border / dim      | 240         | Mid gray          |
| Selected bg       | 236         | Dark gray         |
| Selected fg       | 255         | Bright white      |
| Header            | 75          | Steel blue        |
| Size / accent     | 222         | Gold / yellow     |
| Error             | 196         | Red               |
| Warning           | 214         | Orange            |
| Loading / spinner | 75          | Blue              |
| Description       | 244         | Lighter gray      |
| Directory         | 247         | Slightly dim      |
| Clean ready fg    | 16          | Near-black        |
| Clean ready bg    | 154         | Bright lime green |
| Disabled          | 240         | Muted gray        |

### Style methods

All styling goes through `Theme` methods — never hardcode colors in views:

- `style_normal()` — default text
- `style_selected()` — highlighted row (bold)
- `style_header()` — titles (bold)
- `style_size()` — sizes and accent hotkeys
- `style_border()` — dividers, dim text
- `style_error()` — errors (bold)
- `style_warning()` — warnings
- `style_status_loading()` — spinners, in-progress
- `style_description()` — subtitles, secondary text
- `style_directory()` — directory names
- `style_disabled()` — unavailable actions
- `style_clean_ready()` — CTA button (bold, inverse lime)

---

## Icons & Symbols

### Module icons

Assigned by `module_icon()` in `shared.rs` based on module metadata:

| Icon | Meaning           |
|----- |------------------ |
| 🔨   | Xcode             |
| 📦   | npm / yarn / pnpm |
| 🍺   | Homebrew          |
| 🐳   | Docker            |
| 🗂️   | Cache-related     |
| 📁   | Fallback/unknown  |

### Spinners

- **Scanning / loading**: Braille cycle `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏` — smooth
  rotation at tick rate.
- **Cleanup progress**: ASCII cycle `| / - \` — simpler for progress context.
- Driven by `app.tick_count % CHARS.len()`.

### Other symbols

| Symbol | Usage                       |
|------- |---------------------------- |
| `▶`    | Current row highlight       |
| `│`    | Status bar separator        |
| `[!]`  | Safety warning indicator    |
| `...`  | Size still calculating      |

---

## Loading & Progress States

### Module scanning

- Title bar shows `Scanning... X/Y modules` with a braille spinner.
- Individual items show `...` in the size column until sized.
- Once complete, the spinner disappears and the total size is shown.

### Empty states

- **No modules installed**: Welcome message with instructions to install modules.
- **No items found**: Contextual message based on scan status (scanning, error,
  or genuinely empty).

---

## Size Formatting

Handled by `size_fmt.rs`:

| Range         | Format    | Example   |
|-------------- |---------- |---------- |
| < 1 KB        | `N B`     | `512 B`   |
| < 1 MB        | `N KB`    | `456 KB`  |
| < 1 GB        | `N MB`    | `789 MB`  |
| < 1 TB        | `N.D GB`  | `1.2 GB`  |
| >= 1 TB       | `N.D TB`  | `3.4 TB`  |

GB and TB show one decimal place; smaller units show none.
`format_size_or_placeholder(None)` returns `"..."` for items still being sized.

---

## Checklist for New Screens

When adding a new **screen** (full view the user spends time in), verify:

- [ ] Header with title in `style_header()`
- [ ] Status bar with context-sensitive hotkey hints
- [ ] `[?] help` hotkey registered and visible in status bar
- [ ] Help overlay entry added with all view-specific keybindings
- [ ] All three movement families supported (arrow, vim, emacs)
- [ ] Movement wraps at list boundaries
- [ ] Mouse scroll supported for vertical navigation
- [ ] `/` search if the view contains a filterable list
- [ ] Multi-select with `Space`, `a` (all), `n` (none) if applicable
- [ ] `Esc` / `Backspace` returns to previous view
- [ ] View transition uses `self.set_view()`
- [ ] All colors use `Theme` style methods — no hardcoded colors
- [ ] Hotkey definitions added to `keybindings.rs` with `bar: bool` visibility

## Checklist for New Dialogs

When adding a new **dialog** (transient overlay or call-to-action), verify:

- [ ] Only the keys relevant to the action are handled
- [ ] `Esc` dismisses the dialog
- [ ] Overlay entry uses `app.enter_overlay()`, exit uses `app.leave_overlay()` (preserves scroll position)
- [ ] View transition uses `self.set_view()` only for non-overlay navigation
- [ ] All colors use `Theme` style methods — no hardcoded colors
- [ ] Hotkey definitions added to `keybindings.rs` if a status bar is shown
