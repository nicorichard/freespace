# PRD: Freespace TUI Layer

## Introduction

Freespace is a TUI/CLI application that helps developers reclaim hard drive space through a community-driven module system. This PRD covers the **TUI layer and project bootstrapping** — the interactive terminal interface that lets users browse discovered space consumers, select items for cleanup, and execute deletions with dry-run previews.

The TUI is built with **ratatui + crossterm** on top of an assumed core engine that provides module loading, glob-based discovery, async size calculation, and a deletion engine. This PRD focuses on wiring those capabilities into a responsive, keyboard-driven terminal UI.

## Goals

- Scaffold the Rust project with all required dependencies and directory structure
- Render an interactive module list view sorted by aggregate size
- Provide a detail drill-down view showing individual discovered items per module
- Support keyboard-driven item selection for cleanup
- Show a cleanup confirmation dialog with dry-run mode as the default
- Display a help overlay with keybinding reference
- Stream async size calculation results into the UI in real-time
- Remain responsive while scanning large directory trees

## User Stories

### US-001: Initialize Cargo project with dependencies
**Description:** As a developer, I need a properly configured Rust project so that all required crates are available for building the application.

**Acceptance Criteria:**
- [ ] `Cargo.toml` exists with package name `freespace`, edition 2021
- [ ] Dependencies declared: `ratatui`, `crossterm`, `tokio` (features: full), `clap` (features: derive), `toml`, `serde` (features: derive), `walkdir`, `glob`, `humansize`, `anyhow`, `thiserror`
- [ ] `cargo check` passes with no errors

### US-002: Create project directory structure
**Description:** As a developer, I need the source file layout to match the architecture so that code is organized by subsystem.

**Acceptance Criteria:**
- [ ] Directory structure created matching TECHNICAL_SPECS.md layout:
  - `src/main.rs`, `src/app.rs`, `src/config.rs`
  - `src/tui/mod.rs`, `src/tui/theme.rs`
  - `src/tui/views/mod.rs`, `src/tui/views/module_list.rs`, `src/tui/views/module_detail.rs`, `src/tui/views/cleanup_confirm.rs`, `src/tui/views/help.rs`
  - `src/tui/widgets/mod.rs`
  - `src/core/mod.rs`, `src/core/engine.rs`, `src/core/scanner.rs`, `src/core/cleaner.rs`
  - `src/module/mod.rs`, `src/module/manifest.rs`, `src/module/runtime.rs`, `src/module/manager.rs`
- [ ] Each file contains a placeholder module declaration or minimal stub
- [ ] `cargo check` passes

### US-003: Define core data types and app state
**Description:** As a developer, I need the central `App` struct and supporting types so that all TUI views share a consistent state model.

**Acceptance Criteria:**
- [ ] `App` struct defined in `src/app.rs` with fields: `modules: Vec<ModuleState>`, `current_view: View`, `selected_index: usize`, `selected_items: HashSet<PathBuf>`, `scan_status: ScanStatus`, `config: AppConfig`
- [ ] `View` enum defined with variants: `ModuleList`, `ModuleDetail(usize)`, `CleanupConfirm`, `Help`
- [ ] `ModuleState` struct defined with fields: `module: Module`, `items: Vec<Item>`, `total_size: Option<u64>`, `status: ModuleStatus`
- [ ] `ModuleStatus` enum defined with variants: `Loading`, `Discovering`, `Ready`, `Error(String)`
- [ ] `ScanStatus` enum defined with variants: `Idle`, `Scanning`, `Complete`
- [ ] `Item` struct defined with fields: `name: String`, `path: PathBuf`, `size: Option<u64>`, `item_type: ItemType`
- [ ] `ItemType` enum defined with variants: `File`, `Directory`
- [ ] `cargo check` passes

### US-004: Parse CLI arguments with clap
**Description:** As a user, I want to run `freespace`, `freespace scan`, and `freespace module <subcommand>` so that I can access different modes of the application.

**Acceptance Criteria:**
- [ ] `src/main.rs` uses clap derive to define a `Cli` struct
- [ ] Default command (no subcommand) launches the TUI
- [ ] `scan` subcommand is defined (can be a stub that prints "not yet implemented")
- [ ] `module` subcommand group is defined with sub-subcommands: `install`, `list`, `remove`, `inspect` (all can be stubs)
- [ ] `--help` and `--version` flags work
- [ ] `cargo run -- --help` prints usage information
- [ ] `cargo check` passes

### US-005: Set up terminal initialization and restoration
**Description:** As a developer, I need crossterm terminal setup and teardown so that the TUI enters raw mode on start and restores the terminal cleanly on exit (including panics).

**Acceptance Criteria:**
- [ ] Function to initialize terminal: enable raw mode, enter alternate screen, enable mouse capture, create `CrosstermBackend<Stdout>`
- [ ] Function to restore terminal: disable raw mode, leave alternate screen, disable mouse capture, show cursor
- [ ] Panic hook installed that restores terminal before printing the panic message
- [ ] Terminal is restored on normal exit (quit via `q`)
- [ ] `cargo run` enters and exits alternate screen cleanly

### US-006: Implement main event loop
**Description:** As a developer, I need the central event loop that polls for keyboard input, dispatches to the current view's handler, and triggers re-renders so that the TUI is interactive.

**Acceptance Criteria:**
- [ ] Event loop in `src/app.rs` follows the pattern: poll input → update state → render
- [ ] Uses `crossterm::event::poll` with a tick rate (e.g., 250ms) to allow async updates
- [ ] `q` key exits the application from any view
- [ ] Loop calls the appropriate render function based on `current_view`
- [ ] Loop handles `Event::Resize` to re-render on terminal resize
- [ ] `cargo run` shows a running TUI that can be quit with `q`

### US-007: Implement color theme and styling
**Description:** As a developer, I need a centralized theme definition so that all views use consistent colors and styles.

**Acceptance Criteria:**
- [ ] `src/tui/theme.rs` defines a `Theme` struct with named color fields for: background, foreground, border, selected/highlight, header, size text, error/warning, and module status indicators
- [ ] Default theme uses 256-color palette values that work across Terminal.app, iTerm2, Alacritty, Kitty, and WezTerm
- [ ] Helper functions to create `ratatui::style::Style` values from theme colors (e.g., `theme.style_selected()`, `theme.style_header()`)
- [ ] Theme is accessible from all view render functions via `App` state or passed as parameter
- [ ] `cargo check` passes

### US-008: Render module list view (main screen)
**Description:** As a user, I want to see a list of all detected space consumers with their aggregate sizes so I can understand where my disk space is going.

**Acceptance Criteria:**
- [ ] `src/tui/views/module_list.rs` renders the main view
- [ ] Title bar shows "Freespace" and total aggregate size across all modules
- [ ] Each row displays: module icon (emoji), module name, item count, aggregate size
- [ ] Modules are sorted by total size descending
- [ ] Currently selected row is visually highlighted
- [ ] Modules still calculating size show a spinner or "calculating..." indicator
- [ ] Modules with errors show a warning indicator
- [ ] Status bar at bottom shows keybinding hints: `↑↓ navigate  ⏎ details  c clean  d dry-run  ? help`
- [ ] Layout adapts to terminal width (minimum 80 columns)
- [ ] `cargo run` displays the module list (can use mock data)

### US-009: Keyboard navigation in module list view
**Description:** As a user, I want to navigate the module list using keyboard shortcuts so I can browse efficiently without a mouse.

**Acceptance Criteria:**
- [ ] `j` or `↓` moves selection down one row
- [ ] `k` or `↑` moves selection up one row
- [ ] Navigation wraps: going past the last item wraps to the first, and vice versa
- [ ] `Enter` transitions to the detail view for the selected module
- [ ] `?` opens the help overlay
- [ ] `q` exits the application
- [ ] `c` transitions to cleanup confirmation for all selected items (if any are selected)
- [ ] All keybindings are responsive with no visible lag

### US-010: Render module detail view
**Description:** As a user, I want to drill into a module and see its individual discovered items with sizes so I can make informed cleanup decisions.

**Acceptance Criteria:**
- [ ] `src/tui/views/module_detail.rs` renders the detail view
- [ ] Title bar shows module name, icon, and aggregate size
- [ ] Each row displays: selection checkbox (`[ ]` or `[x]`), item name, item size
- [ ] Items are sorted by size descending
- [ ] Currently selected row is visually highlighted
- [ ] Items still calculating size show "calculating..."
- [ ] Items with size calculation errors show "N/A" with a warning indicator
- [ ] Status bar shows keybinding hints: `↑↓ navigate  ␣ select  a all  n none  ⏎ clean  ⌫ back`
- [ ] Layout adapts to terminal width

### US-011: Keyboard navigation and selection in detail view
**Description:** As a user, I want to select specific items for cleanup using keyboard shortcuts so I can keep what I need and remove what I don't.

**Acceptance Criteria:**
- [ ] `j` or `↓` moves selection down one row
- [ ] `k` or `↑` moves selection up one row
- [ ] Navigation wraps at list boundaries
- [ ] `Space` toggles the selection checkbox on the currently highlighted item
- [ ] `a` selects all items in the current module
- [ ] `n` deselects all items in the current module
- [ ] `Enter` or `c` transitions to cleanup confirmation (if any items are selected)
- [ ] `Backspace` or `Esc` returns to the module list view
- [ ] `?` opens the help overlay
- [ ] `q` exits the application
- [ ] Selected items are tracked in `App.selected_items` (persists when navigating back and forth)

### US-012: Render cleanup confirmation view
**Description:** As a user, I want to preview what will be deleted before confirming so I don't accidentally remove important data.

**Acceptance Criteria:**
- [ ] `src/tui/views/cleanup_confirm.rs` renders as a centered modal/dialog
- [ ] Header reads "Cleanup Preview"
- [ ] Lists each selected item with its path and size
- [ ] Shows total count of items and total size to be reclaimed
- [ ] Displays current mode prominently: "DRY RUN (no files will be deleted)" or "LIVE — files will be permanently deleted"
- [ ] Shows two action buttons: "Confirm (y)" and "Cancel (n)"
- [ ] If the list of items is long, it scrolls within the dialog
- [ ] Layout is centered and does not exceed 80% of terminal width/height

### US-013: Keyboard actions in cleanup confirmation view
**Description:** As a user, I want to confirm or cancel the cleanup action with simple keypresses.

**Acceptance Criteria:**
- [ ] `y` confirms the cleanup action
- [ ] `n` or `Esc` cancels and returns to the previous view
- [ ] In dry-run mode: confirming displays a summary of what *would* be deleted, then returns to previous view
- [ ] In live mode: confirming triggers deletion via the core engine, displays progress, then shows a results summary (items deleted, space reclaimed, any errors)
- [ ] `q` exits the application (with an "are you sure?" if items are pending cleanup — or just cancels and returns)

### US-014: Render help overlay
**Description:** As a user, I want to see a keybinding reference by pressing `?` so I can learn the available shortcuts.

**Acceptance Criteria:**
- [ ] `src/tui/views/help.rs` renders as a centered overlay/popup on top of the current view
- [ ] Lists all keybindings organized by context: Global, Module List, Module Detail, Cleanup
- [ ] Each entry shows the key(s) and their action (e.g., `j/↓  Move down`)
- [ ] `?` or `Esc` closes the help overlay and returns to the underlying view
- [ ] Overlay is visually distinct (e.g., different background color or border style)

### US-015: Integrate async size calculation with TUI
**Description:** As a developer, I need the TUI to receive streaming size updates from the core scanner so that the UI refreshes as calculations complete.

**Acceptance Criteria:**
- [ ] A `tokio::sync::mpsc` channel is created for size update messages
- [ ] The core scanner sends `(module_index, item_index, size: u64)` messages as each item's size is calculated
- [ ] The event loop checks the channel receiver on each tick and updates `ModuleState.items[i].size` and `ModuleState.total_size`
- [ ] Updated sizes are reflected on the next render cycle
- [ ] Module rows in the list view update their aggregate size in real-time
- [ ] When all calculations complete, `scan_status` transitions to `Complete`
- [ ] UI remains responsive (no blocking) during size calculation

### US-016: Format and display human-readable sizes
**Description:** As a user, I want sizes displayed in human-readable format (e.g., "12.3 GB") so I can quickly understand the scale.

**Acceptance Criteria:**
- [ ] Utility function in `src/tui/widgets/` or a shared module that formats `u64` bytes into human-readable strings
- [ ] Uses appropriate units: B, KB, MB, GB, TB
- [ ] Shows one decimal place for GB and above (e.g., "12.3 GB"), no decimals for MB and below (e.g., "847 MB")
- [ ] Items with `None` size display "..." or a spinner character
- [ ] Items with size errors display "N/A"
- [ ] `cargo test` includes unit tests for the formatting function

### US-017: Display scan progress indicator
**Description:** As a user, I want to see that scanning is in progress so I know the application is working and not frozen.

**Acceptance Criteria:**
- [ ] When `scan_status` is `Scanning`, a progress indicator is visible in the title bar or status bar
- [ ] Indicator shows how many modules/items have completed size calculation vs total (e.g., "Scanning... 3/5 modules")
- [ ] Indicator animates (e.g., spinner character cycles) so the UI looks alive
- [ ] When `scan_status` transitions to `Complete`, the indicator is replaced with the final total size

### US-018: Handle terminal resize
**Description:** As a user, I want the TUI to adapt when I resize my terminal window so that the layout doesn't break.

**Acceptance Criteria:**
- [ ] `Event::Resize` triggers an immediate re-render
- [ ] All views recalculate their layout based on the new terminal dimensions
- [ ] Minimum supported width is 80 columns; if terminal is narrower, a message is shown
- [ ] No panics or rendering artifacts on resize

### US-019: Create built-in module TOML manifests
**Description:** As a user, I want Freespace to ship with modules for common developer tools so it's useful immediately without installing community modules.

**Acceptance Criteria:**
- [ ] `modules/xcode-derived-data/module.toml` — targets `~/Library/Developer/Xcode/DerivedData/*` (platform: macos)
- [ ] `modules/npm-cache/module.toml` — targets npm cache dir, `~/.yarn/cache`, pnpm store (platform: macos, linux)
- [ ] `modules/homebrew-cache/module.toml` — targets `~/Library/Caches/Homebrew/*` (platform: macos)
- [ ] `modules/docker/module.toml` — targets Docker Desktop disk images and build cache (platform: macos, linux)
- [ ] `modules/general-caches/module.toml` — targets `~/Library/Caches/*` and `~/.cache/*` (platform: macos, linux)
- [ ] All manifests conform to the `module.toml` schema defined in TECHNICAL_SPECS.md
- [ ] All manifests include: name, version, description, author, platforms, at least one `[[targets]]` entry

### US-020: Load built-in modules on startup
**Description:** As a developer, I need the application to discover and load built-in modules from the `modules/` directory so they are available for scanning without installation.

**Acceptance Criteria:**
- [ ] On startup, the app scans a known path for built-in modules (embedded in binary or relative to executable)
- [ ] Each `module.toml` is parsed and deserialized into a `Module` struct
- [ ] Modules for unsupported platforms are filtered out (e.g., skip macos-only modules on Linux)
- [ ] Parsing errors for individual modules are logged as warnings but do not crash the app
- [ ] Successfully loaded modules populate `App.modules` as `ModuleState` entries
- [ ] `cargo check` passes

## Functional Requirements

- FR-1: The application must enter an alternate terminal screen using crossterm raw mode on startup and restore the terminal fully on exit (including panics)
- FR-2: The default command (no subcommand) must launch the interactive TUI
- FR-3: The module list view must display all loaded modules sorted by aggregate size descending
- FR-4: Each module row must show: icon, name, item count, and human-readable aggregate size
- FR-5: The user must be able to navigate lists using `j`/`k` or `↑`/`↓` arrow keys
- FR-6: Pressing `Enter` on a module in the list view must open its detail view
- FR-7: The detail view must display individual discovered items with selection checkboxes and sizes
- FR-8: `Space` must toggle selection on the highlighted item; `a` selects all; `n` deselects all
- FR-9: The cleanup confirmation view must list all selected items, their sizes, and the total
- FR-10: The cleanup confirmation view must prominently display whether the mode is dry-run or live
- FR-11: Dry-run mode must be the default; no files are deleted unless the user explicitly switches to live mode
- FR-12: `y` in the confirmation view must execute the cleanup; `n` or `Esc` must cancel
- FR-13: The help overlay must be accessible via `?` from any view and dismissed with `?` or `Esc`
- FR-14: Size calculations must run asynchronously and stream results into the UI without blocking
- FR-15: The TUI must handle terminal resize events without crashing or layout corruption
- FR-16: All sizes must be displayed in human-readable format (B, KB, MB, GB, TB)
- FR-17: The application must ship with 5 built-in modules: Xcode DerivedData, npm/yarn/pnpm cache, Homebrew cache, Docker, and general caches
- FR-18: `q` must exit the application from any view

## Non-Goals (Out of Scope)

- Core engine implementation (scanning, deletion logic) — assumed to exist
- Module manager CLI (install, remove, list, inspect) — separate PRD
- Dynamic Lua scripting for modules
- Non-interactive / headless mode (`freespace clean --all`)
- Trash integration (move to trash instead of permanent delete)
- Shell completions
- Configuration profiles
- Module registry / search
- Disk usage trends / history tracking
- Mouse input support
- Custom user themes (only default theme for now)

## Technical Considerations

- **Framework:** ratatui v0.28+ with crossterm backend
- **Async runtime:** tokio for spawning size calculation tasks; results sent via `mpsc` channel and merged into the event loop on each tick
- **State management:** single `App` struct owned by the event loop; views receive `&App` for rendering and `&mut App` for updates
- **Built-in modules:** embedded via `include_str!` or loaded from a `modules/` directory relative to the binary
- **Terminal compatibility:** must work in Terminal.app, iTerm2, Alacritty, Kitty, WezTerm — avoid truecolor-only features, use 256-color as baseline
- **Minimum terminal size:** 80 columns wide, 24 rows tall

## Success Metrics

- TUI is visible within 500ms of launch
- Module list renders with streaming size updates — no frozen UI during scanning
- User can navigate, select items, and trigger cleanup entirely via keyboard
- Terminal is always restored cleanly on exit, including after panics
- All 5 built-in modules load and display correctly on macOS

## Open Questions

- Should the detail view support multi-column layout for wide terminals?
- Should there be a "toggle dry-run / live mode" keybinding, or should live mode only be available via a CLI flag (e.g., `freespace --live`)?
- Should the module list view show a "last cleaned" timestamp per module?
- How should the TUI handle extremely long item names — truncate with ellipsis or horizontal scroll?
