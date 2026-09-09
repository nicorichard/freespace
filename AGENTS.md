If you discover a **reusable pattern** that future iterations should know, add it to the `## Codebase Patterns` section. Only add patterns that are **general and reusable**, not story-specific details.

## Project overview

**freespace** is a Rust TUI application for browsing and cleaning disk space consumers.
Built with ratatui + crossterm + tokio. Users navigate declarative TOML module manifests
that define glob patterns to discover large files/directories, then selectively trash or
delete them.

## Build / test / lint

```sh
cargo build --release   # release binary
cargo build             # debug build
cargo test              # run tests
cargo clippy            # lint
cargo check             # type-check without building
```

No CI pipelines are configured. Edition 2021.

## Directory layout

```
catalog/               Built-in module manifests, embedded at compile time by build.rs.
src/
  main.rs              CLI entry point (clap). Parses args, boots tokio runtime, launches TUI.
  app/
    mod.rs             Central App struct: state, event loop, key handling, View dispatch.
    types.rs           Item, ModuleState, View, and other shared types.
    drill.rs           Drill-in (directory browsing) state.
    filter.rs          Search/filter matching.
  config.rs            User config (~/.config/freespace/config.toml).
  core/
    scanner.rs         Async filesystem scanner. Sends ScanMessage variants over mpsc channel.
    cleaner.rs         Trash (via `trash` crate) and permanent delete logic.
    safety.rs          Path deny/warn classification.
    audit.rs           JSONL audit log of cleanup operations.
    stats.rs           Reclaimed-space statistics.
    paths.rs           Tilde expansion.
    handlers/
      mod.rs           Handler trait + the closed registry manifests select from.
      exec.rs          The only subprocess entry point (argv only, never a shell).
      xcode.rs         Simulator device and runtime handlers via simctl.
  module/
    manifest.rs        TOML manifest parsing into Module struct.
    catalog.rs         Loads the built-in catalog embedded from catalog/.
    installer.rs       Git-based module installation.
    manager.rs         Module discovery, loading, and catalog/user precedence.
    source.rs          Module source types (local, git).
  tui/
    theme.rs           Color palette and style constants.
    views/
      module_list.rs   Main view: list of modules with sizes.
      module_detail.rs Detail view: items within a module, drill-in support.
      cleanup_confirm.rs Confirmation dialog before trash/delete.
      cleanup_results.rs Post-cleanup failures with their error text.
      help.rs          Help overlay.
    widgets/
      size_fmt.rs      Human-readable size formatting.
      shared.rs        Reusable widget helpers.
```

## Architecture

Event-driven TUI with background async scanning:

1. `main.rs` parses CLI args, loads modules, spawns the `App` event loop.
2. `App` (app.rs) owns all state. It reads crossterm key events and `ScanMessage`s
   from a tokio mpsc channel in a unified select loop.
3. `scanner.rs` runs async tasks per module — walks directories, calculates sizes,
   sends `ScanMessage` variants (`ItemDiscovered`, `ModuleComplete`, `ModuleError`,
   `DrillItemSized`).
4. View enum dispatches rendering: `ModuleList` -> `ModuleDetail(index)` ->
   `CleanupConfirm` -> `CleanupProgress` -> `CleanupResults` (only when
   something failed), with `Help` as an overlay.

## Module system

freespace ships a built-in catalog (`catalog/`, embedded by `build.rs`), so it is
complete without installing anything. Installed TOML modules are a secondary
extension point.

- Built-in: `catalog/<name>/module.toml`, disabled via `[modules]` in config.toml
- User modules: `~/.config/freespace/modules/` plus `module_dirs`
- A user module with the same `id` as a built-in replaces it
- Git-distributable: `freespace module install <source>`
- Platform-filtered via `platforms` field (macos/linux/windows)

Manifests are declarative and cannot specify commands. A target declares either
path patterns or the name of a **handler** — a compiled-in integration selected
from a closed set (`core::handlers`), rejected at parse time if unknown. Handlers
exist for cleanup that isn't path-shaped: `xcrun simctl delete <UDID>` rather than
trashing a CoreSimulator directory and corrupting its registry.

## UI Design Guide

All TUI changes **must** follow [`docs/UI_DESIGN_GUIDE.md`](docs/UI_DESIGN_GUIDE.md).
Read it before modifying any view, widget, or keybinding. Key rules:

- Every screen needs a `[?] help` hint and status bar with hotkey hints
- All scrollable surfaces support arrow keys, vim keys (`j`/`k`/`h`/`l`), and emacs keys (`Ctrl+N`/`P`/`F`/`B`)
- Large lists must support `/` search
- Multi-selectable interfaces need `Space` (toggle), `a` (all), `n` (none)
- Dialogs favour movement (navigable lists) over hotkeys
- All colors go through `Theme` style methods — never hardcode
- Use `self.set_view()` for view transitions, never direct assignment
- Add hotkey definitions to `keybindings.rs` for new actions

## Conventions

- `anyhow` for error propagation, `thiserror` for typed errors in core
- `eprintln!` for warnings; no structured logging
- Minimal test coverage (size_fmt unit tests)
- No CI/CD pipelines

## Quality Requirements

- ALL work cannot be considered complete until it passes quality checks
  - Formatted with `cargo fmt`
  - Compiles without warnings
  - Passes all tests
- Keep changes focused and minimal
- Follow existing code patterns

## Codebase Patterns

- **Cross-platform `libc` casts:** `libc::statvfs` fields differ in type across platforms (e.g. `u32` on macOS, `u64` on Linux). Use `#[allow(clippy::unnecessary_cast)]` on functions that cast these fields to `u64` so clippy passes on all targets.
- **View-based navigation:** Directory browsing always uses `View::FileBrowser`. New views that need directory browsing should transition to FileBrowser with `browser_origin` set — never embed drill logic inline.
- **View transitions:** Always use `self.set_view(view)` instead of direct `self.current_view = view` assignment — it resets `view_offset` automatically to prevent stale scroll positions.
- **Handler items are not files:** Items produced by a `core::handlers` handler are keyed by a synthetic *relative* path (`freespace-handler/<handler>/<id>`) so they flow through the existing `BTreeSet<PathBuf>` selection machinery without ever colliding with a real target path. Their real location, if any, is `Item::display_path` and is display-only. `Item::action` — never the path's shape — decides how an item is removed, and handler actions deliberately skip `check_safety` because path safety answers the wrong question for an operation that unlinks nothing. Handlers also supply their own sizes, so these items skip the scanner's sizing pass.
- **Adding a field to `Item`, `Target`, or `ModuleState`:** these are constructed literally in ~8 test fixtures. `Item` and `Target` derive `Default`, so fixtures use `..Default::default()` — prefer adding a defaulted field over touching every fixture.
- **`cleanup_confirm.rs` sub-rows:** any new sub-line must be added to *both* `render_items_list` and `visual_row_to_item_index`, in the same order, or mouse-click mapping silently desyncs from what's on screen.
