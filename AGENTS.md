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
cargo test              # run tests (minimal: size_fmt only)
cargo clippy            # lint
cargo check             # type-check without building
```

No CI pipelines are configured. Edition 2021.

## Directory layout

```
src/
  main.rs              CLI entry point (clap). Parses args, boots tokio runtime, launches TUI.
  app.rs               Central App struct: state, event loop, key handling, View dispatch.
  config.rs            User config (~/.config/freespace/config.toml).
  core/
    scanner.rs         Async filesystem scanner. Sends ScanMessage variants over mpsc channel.
    cleaner.rs         Trash (via `trash` crate) and permanent delete logic.
    engine.rs          Orchestrates scan tasks per module.
  module/
    manifest.rs        TOML manifest parsing into Module struct.
    installer.rs       Git-based module installation.
    manager.rs         Module discovery and loading from config dir.
    runtime.rs         Runtime resolution of module patterns.
    source.rs          Module source types (local, git).
  tui/
    theme.rs           Color palette and style constants.
    views/
      module_list.rs   Main view: list of modules with sizes.
      module_detail.rs Detail view: items within a module, drill-in support.
      cleanup_confirm.rs Confirmation dialog before trash/delete.
      help.rs          Help overlay.
    widgets/
      size_fmt.rs      Human-readable size formatting.
      shared.rs        Reusable widget helpers.
modules/               Bundled TOML module manifests (docker, npm-cache, xcode, etc.)
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
   `CleanupConfirm`, with `Help` as an overlay.

## Module system

Modules are declarative TOML manifests — no code execution.

- Default location: `~/.config/freespace/modules/`
- Bundled defaults in `modules/` directory (installed on first run)
- Git-distributable: `freespace install <git-url>`
- Platform-filtered via `platform` field (macos/linux/windows)
- Use `glob` patterns and `local_target` paths to discover items

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
