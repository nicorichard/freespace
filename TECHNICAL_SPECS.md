# Freespace — Technical Specification

## Architecture Overview

Freespace is composed of four main subsystems:

```
┌─────────────────────────────────────────────────────┐
│                    TUI Layer                         │
│              (ratatui + crossterm)                   │
├─────────────────────────────────────────────────────┤
│                   Core Engine                        │
│         (scanning, deletion, orchestration)          │
├──────────────────────┬──────────────────────────────┤
│   Module Runtime     │     Module Manager           │
│   (TOML parser)      │  (git clone, versioning)     │
└──────────────────────┴──────────────────────────────┘
```

- **Core Engine** — orchestrates module loading, discovery execution, size calculation, and cleanup. All filesystem writes and deletes flow through the core.
- **Module Runtime** — parses `module.toml` manifests and resolves glob-based targets. Produces item lists for the core.
- **TUI Layer** — renders the interactive terminal UI, handles input, and communicates user actions back to the core.
- **Module Manager** — handles module installation and removal via git.

## Technology Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Language | Rust (latest stable) | Core binary |
| TUI framework | ratatui + crossterm | Terminal rendering and input |
| Config/manifest parsing | toml | TOML deserialization |
| Async runtime | tokio | Async filesystem operations |
| CLI argument parsing | clap (derive) | Command-line interface |
| Serialization | serde | Data structure (de)serialization |
| Filesystem walking | walkdir / ignore | Recursive directory traversal |
| Git operations | std::process::Command (git) | Module installation via git clone |
| Glob matching | glob | Pattern-based path matching |
| Human-readable sizes | humansize | Formatting byte counts |
| Error handling | anyhow / thiserror | Application and library errors |

## Module Specification

### Directory Structure

A module is a directory (typically a git repository) containing:

```
my-module/
└── module.toml          # Required: module manifest
```

### `module.toml` Manifest Format

```toml
[module]
name = "xcode-derived-data"
version = "1.0.0"
description = "Xcode DerivedData build artifacts"
author = "freespace-contrib"
license = "MIT"
platforms = ["macos"]          # "macos", "linux", or both
min_freespace_version = "0.1.0"

[module.metadata]
category = "ide"               # ide, package-manager, container, cache, build, other
icon = "🔨"                    # Optional: emoji for TUI display
url = "https://github.com/user/freespace-xcode-derived-data"

# Static discovery — glob patterns resolved by the core engine
[[targets]]
name = "DerivedData Projects"
description = "Per-project Xcode build artifacts"
path = "~/Library/Developer/Xcode/DerivedData/*"
item_type = "directory"        # "directory" or "file"
# Optional: exclude patterns
exclude = ["ModuleCache"]

[[targets]]
name = "DerivedData Module Cache"
description = "Shared Clang module cache"
path = "~/Library/Developer/Xcode/DerivedData/ModuleCache.noindex"
item_type = "directory"
```

### Manifest Field Reference

#### `[module]` — Required

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Unique module identifier (lowercase, hyphens) |
| `version` | string | yes | Semver version |
| `description` | string | yes | Short description (shown in TUI) |
| `author` | string | yes | Author name or org |
| `license` | string | no | SPDX license identifier |
| `platforms` | array | yes | Supported platforms |
| `min_freespace_version` | string | no | Minimum compatible Freespace version |

#### `[[targets]]` — Required (at least one target)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Display name for this target |
| `description` | string | no | Explanation shown in detail view |
| `path` | string | yes | Glob pattern (`~` expanded, `*`/`**` supported) |
| `item_type` | string | yes | `"directory"` or `"file"` |
| `exclude` | array | no | Glob patterns to exclude from matches |

### Versioning

Modules use semantic versioning (semver). The `version` field in `module.toml` is the source of truth. Git tags (e.g., `v1.0.0`) are used for installation pinning.

### Future Extensibility: Scan Scope

The module manifest is designed to accommodate a future `scope` field on `[module]`
(e.g., `scope = "global"` vs `scope = "local"`). Currently all modules are implicitly
global — they scan fixed, absolute paths. In the future:

- Local modules would use relative patterns (e.g., `**/node_modules`) resolved against
  a user-provided scan root
- The CLI would support `freespace <path>` for ad-hoc local scans
- A project directory registry in config.toml would allow persistent roots

No manifest changes are needed now. The `[[targets]]` path field already supports globs,
and adding scope-aware resolution is additive.

## Future: Dynamic Discovery

A sandboxed Lua scripting layer is planned for modules that need discovery logic beyond static glob patterns (e.g., reading Docker's `daemon.json` to find custom data directories). This will include a curated read-only filesystem API, resource limits, and a restricted Lua 5.4 environment. Until then, all modules use declarative `[[targets]]` glob patterns.

## Module Lifecycle

### 1. Install

```
freespace module install github:user/freespace-xcode@v1.0.0
```

1. Parse the source identifier (GitHub org/repo + optional tag)
2. Clone the repository (shallow clone, specific tag if provided) to a temporary directory
3. Validate: check `module.toml` exists and parses correctly
4. If a module with the same name is already installed, replace it (upgrade in place)
5. Move module directory to `~/.config/freespace/modules/<name>/`
6. Register (or update) module in `~/.config/freespace/config.toml`

### 2. Load

On startup, for each registered module:

1. Read `module.toml` from the module directory
2. Deserialize into a `Module` struct
3. Check platform compatibility (skip if current platform not in `platforms`)
4. Add module to the active module list

### 3. Discover

For each loaded module, discovery produces a list of items:

1. For each `[[targets]]` entry, expand `~` and resolve glob patterns
2. Filter by `exclude` patterns
3. Produce an `Item` for each match

### 4. Size Calculation

After discovery produces item lists:

1. For each item, spawn an async task to calculate size
2. Directory sizes are calculated recursively using `walkdir`
3. Results stream back to the TUI as they complete
4. Items with permission errors show "N/A" with a warning indicator
5. Module aggregate sizes update in real-time

### 5. Display

The TUI renders discovered items:

1. Main view shows modules sorted by total size (descending)
2. Each module row: icon, name, item count, aggregate size
3. Detail view (on Enter): list of individual items with sizes
4. Items can be selected/deselected for cleanup

### 6. Cleanup

When the user triggers cleanup:

1. Collect all selected items across modules
2. Display a confirmation dialog listing items and total size
3. In dry-run mode (default): show what would be deleted and exit
4. In live mode: for each item, call the core deletion engine
5. Deletion engine removes files/directories (recursively for directories)
6. Report results: items deleted, space reclaimed, any errors

## Security Model

### Threat Model

Modules are community-contributed code. The security model assumes modules may be malicious or buggy and constrains them accordingly.

### Sandboxing Layers

**Layer 1 — Declarative manifests:** All modules are pure TOML. They declare paths and patterns. The core engine resolves them. No code execution involved.

**Layer 2 — Core-controlled deletion:** Modules never delete anything. They only produce item lists. All deletion is performed by the core engine, gated by user confirmation.

> **Future:** When Lua-based dynamic discovery is added, a sandbox layer will be introduced between layers 1 and 2, providing a restricted Lua environment with resource limits and scoped filesystem access.

### Module Audit

- Module manifests are small, human-readable TOML files
- `freespace module inspect <name>` shows the full manifest contents
- On install, a summary of the module's targets is displayed for review
- No binary or compiled code is allowed in modules

### Filesystem Safety

- The core deletion engine refuses to delete paths outside the user's home directory
- Protected paths (e.g., `~/.ssh`, `~/.gnupg`) are on a blocklist
- Symlinks are not followed during deletion (only the link is removed)

## Module Installation & Management

### Source Identifiers

```
github:user/repo              # Latest default branch
github:user/repo@v1.0.0      # Specific git tag
github:user/repo@branch       # Specific branch (discouraged for stability)
```

### Storage Layout

```
~/.config/freespace/
├── config.toml               # Global config + module registry
└── modules/
    ├── xcode-derived-data/
    │   └── module.toml
    ├── npm-cache/
    │   └── module.toml
    └── docker/
        └── module.toml
```

### `config.toml` Format

```toml
[settings]
dry_run = true                     # Default: dry-run mode enabled
theme = "default"                  # TUI color theme

[[modules]]
name = "xcode-derived-data"
source = "github:freespace-contrib/xcode-derived-data@v1.0.0"
enabled = true
installed_at = "2026-01-15T10:30:00Z"

[[modules]]
name = "npm-cache"
source = "builtin"
enabled = true
```

### CLI Commands

| Command | Description |
|---------|-------------|
| `freespace module install <source>` | Install or upgrade a module from GitHub |
| `freespace module remove <name>` | Uninstall a module |
| `freespace module list` | List installed modules with versions and status |
| `freespace module inspect <name>` | Show full manifest contents |

## Core Data Flow

```
                                   ┌───────────────┐
                                   │  module.toml  │
                                   └──────┬────────┘
                                          │ parse
                                          ▼
                               ┌──────────────────────┐
                               │    Module Runtime     │
                               │                      │
                  ┌────────────┤  Static: glob resolve │
                  │            │                      │
                  │            └──────────┬───────────┘
                  │                       │ item list
                  │                       ▼
                  │            ┌──────────────────────┐
                  │            │    Size Calculator    │
                  │            │  (async, per-item)    │
                  │            └──────────┬───────────┘
                  │                       │ sized items
                  │                       ▼
                  │            ┌──────────────────────┐
                  │            │      TUI Layer        │
                  │            │  List → Detail → Act  │
                  │            └──────────┬───────────┘
                  │                       │ user selection
                  │                       ▼
                  │            ┌──────────────────────┐
                  │            │   Deletion Engine     │
                  │            │  (confirm → execute)  │
                  │            └──────────────────────┘
```

## TUI Architecture

### Views

**Main View (Module List)**
```
┌─ Freespace ──────────────────── 48.2 GB total ─┐
│                                                  │
│  🔨 Xcode DerivedData          12.3 GB   14 items│
│  📦 npm/yarn/pnpm cache         8.7 GB    3 items│
│  🐳 Docker                      6.2 GB    5 items│
│  🍺 Homebrew cache              4.1 GB    1 item │
│  📁 General caches              2.8 GB   42 items│
│                                                  │
├──────────────────────────────────────────────────┤
│ ↑↓ navigate  ⏎ details  c clean  d dry-run  ? help│
└──────────────────────────────────────────────────┘
```

**Detail View (Module Items)**
```
┌─ Xcode DerivedData ──────────────── 12.3 GB ───┐
│                                                  │
│  [ ] MyApp-abc123               3.2 GB          │
│  [x] OldProject-def456         2.8 GB          │
│  [x] Playground-ghi789         1.1 GB          │
│  [ ] SharedFramework-jkl012    0.9 GB          │
│  ...                                            │
│                                                  │
├──────────────────────────────────────────────────┤
│ ↑↓ navigate  ␣ select  a all  ⏎ clean  ⌫ back  │
└──────────────────────────────────────────────────┘
```

**Action View (Cleanup Confirmation)**
```
┌─ Cleanup Preview ───────────────────────────────┐
│                                                  │
│  The following items will be DELETED:            │
│                                                  │
│  • OldProject-def456         2.8 GB             │
│  • Playground-ghi789         1.1 GB             │
│                                                  │
│  Total: 3.9 GB from 2 items                     │
│                                                  │
│  ┌─────────────────┐  ┌──────────────────┐      │
│  │   Confirm (y)    │  │   Cancel (n)     │      │
│  └─────────────────┘  └──────────────────┘      │
│                                                  │
│  Mode: DRY RUN (no files will be deleted)        │
└──────────────────────────────────────────────────┘
```

### Event Loop

The TUI follows the standard ratatui event loop pattern:

1. **Input** — crossterm polls for keyboard/mouse events
2. **Update** — events dispatched to the current view's handler, state updated
3. **Render** — current view renders from state using ratatui widgets
4. **Async** — tokio tasks for size calculation send updates via channels; these are merged into the event loop

### State Management

```rust
struct App {
    modules: Vec<ModuleState>,       // Loaded modules with discovery results
    current_view: View,              // Which view is active
    selected_index: usize,           // Cursor position in current list
    selected_items: HashSet<PathBuf>,// Items marked for cleanup
    scan_status: ScanStatus,         // Idle, Scanning, Complete
    config: AppConfig,               // User settings
}

enum View {
    ModuleList,
    ModuleDetail(usize),             // Index into modules
    CleanupConfirm,
    Help,
}

struct ModuleState {
    module: Module,                  // Parsed module definition
    items: Vec<Item>,                // Discovered items
    total_size: Option<u64>,         // Aggregate size (None if still calculating)
    status: ModuleStatus,            // Loading, Discovering, Ready, Error
}
```

## Project Structure

```
freespace/
├── Cargo.toml
├── src/
│   ├── main.rs                     # Entry point, CLI parsing
│   ├── app.rs                      # App state and event loop
│   ├── tui/
│   │   ├── mod.rs
│   │   ├── views/
│   │   │   ├── module_list.rs      # Main view
│   │   │   ├── module_detail.rs    # Detail view
│   │   │   ├── cleanup_confirm.rs  # Confirmation dialog
│   │   │   └── help.rs             # Help overlay
│   │   ├── widgets/                # Custom ratatui widgets
│   │   └── theme.rs                # Colors and styling
│   ├── core/
│   │   ├── mod.rs
│   │   ├── engine.rs               # Orchestration logic
│   │   ├── scanner.rs              # Async size calculation
│   │   └── cleaner.rs              # Deletion engine
│   ├── module/
│   │   ├── mod.rs
│   │   ├── manifest.rs             # TOML parsing and validation
│   │   ├── runtime.rs              # Module loading and lifecycle
│   │   └── manager.rs              # Install, remove
│   └── config.rs                   # Config file handling
├── modules/                        # Built-in modules
│   ├── xcode-derived-data/
│   │   └── module.toml
│   ├── npm-cache/
│   │   └── module.toml
│   ├── homebrew-cache/
│   │   └── module.toml
│   ├── docker/
│   │   └── module.toml
│   └── general-caches/
│       └── module.toml
├── VISION.md
├── PRD.md
└── TECHNICAL_SPECS.md
```
