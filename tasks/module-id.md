# Task: Module `id` Field

## Goal

Add an `id` field to module TOML manifests and use it as the canonical identifier for CLI operations, directory naming, and internal lookups.

## Why

The current `name` field (e.g., `"Node Modules"`) is human-readable but awkward for CLI usage: `freespace module remove "Node Modules"` requires quoting and doesn't match the filesystem directory structure. An `id` field like `node-modules` provides a slug that works everywhere.

## How

### 1. Add `id` field to Module struct

In `src/module/manifest.rs`, add an `id` field:

```rust
pub struct Module {
    pub id: String,             // NEW: kebab-case slug, e.g. "node-modules"
    pub name: String,           // Display name, e.g. "Node Modules"
    pub version: String,
    pub description: String,
    pub author: String,
    pub platforms: Vec<String>,
    pub targets: Vec<Target>,
}
```

### 2. Update all bundled manifests

Add `id = "..."` to every TOML file in `modules/`:

```toml
id = "docker"
name = "Docker"
```

```toml
id = "node-modules"
name = "Node Modules"
```

Use kebab-case, matching the TOML filename without extension.

### 3. Validation

In manifest parsing, validate:
- `id` is non-empty
- `id` matches `^[a-z0-9]+(-[a-z0-9]+)*$` (kebab-case)
- `id` is unique across loaded modules (check at load time in `manager.rs`)

### 4. Update CLI identification

In `src/main.rs`, module subcommands (`remove`, `inspect`) should accept the `id` instead of `name`:

```bash
freespace module remove node-modules    # Uses id
freespace module inspect docker         # Uses id
```

### 5. Update module directory naming

When installing modules, use `id` as the directory name under `~/.config/freespace/modules/`. This makes the filesystem match the CLI identifier.

## UX

Before:
```bash
freespace module remove "Node Modules"
```

After:
```bash
freespace module remove node-modules
```

## Verification

- [ ] All bundled TOML manifests have an `id` field
- [ ] `id` validation rejects invalid slugs (empty, uppercase, spaces)
- [ ] `freespace module list` shows both id and name
- [ ] `freespace module remove <id>` works
- [ ] `freespace module inspect <id>` works
- [ ] Installed module directories match the `id`
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

Nothing — foundational schema change.

## Blocks

- `info-pane.md` — info pane displays module id and metadata
- `install-selection-ux.md` — install flow uses id for module identification
