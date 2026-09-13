# Writing Freespace Modules

This document is for the optional case: writing your own module. Freespace ships a built-in catalog and works out of the box with nothing installed — a module is how you add paths the catalog doesn't cover, or override a built-in.

Modules are declarative TOML files. A module is a directory containing a `module.toml` manifest that declares either **path patterns** to scan, or the name of a **built-in handler**. A manifest cannot specify a command — it can only select from the fixed set of handlers compiled into freespace (see [Handlers](#handlers)), so installing a module never introduces new code.

## Built-in modules

Everything in the catalog loads automatically. To see what you have:

```sh
freespace module list          # built-ins are marked "built-in"
freespace module inspect docker
```

Turn one off, or back on:

```sh
freespace module disable steam
freespace module enable steam
```

Both edit `[modules]` in `~/.config/freespace/config.toml`:

```toml
[modules]
builtin = true                  # set false to disable the whole catalog
disabled = ["steam", "spotify"]
```

Installing a module with the same `id` as a built-in **replaces** it, which is the supported way to customise a shipped module: copy its manifest, edit it, and install it locally.

## Quick Start

1. Create a directory for your module:

```
mkdir my-module
```

2. Write a `module.toml` inside it:

```toml
name = "My Module"
version = "1.0.0"
description = "Scans for something. Safe to delete."
author = "yourname"
platforms = ["macos"]

[[targets]]
path = "~/some/path"
description = "What lives here"
```

3. Drop the directory into `~/.config/freespace/modules/`:

```
mkdir -p ~/.config/freespace/modules
cp -r my-module ~/.config/freespace/modules/
```

4. Run `freespace` — your module appears in the TUI.

## Manifest Reference

Every module must have a `module.toml` at the root of its directory. All top-level fields are required.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | Yes | Display name shown in the TUI |
| `version` | String | Yes | Semver version (e.g. `"1.0.0"`) |
| `description` | String | Yes | What this module scans and whether it's safe to delete |
| `author` | String | Yes | Author name or handle |
| `platforms` | Array of strings | Yes | Supported platforms: `"macos"`, `"linux"`, `"windows"` |
| `icon` | String | No | Nerd Font glyph — either a literal character or `U+XXXX`/`U+XXXXX` escape (must be in PUA range) |
| `icon_color` | String | No | Hex color for the icon (e.g. `"#CE422B"`) |
| `[[targets]]` | Array of tables | Yes | At least one target required |
| `targets.path` | String or array | * | Path pattern (supports `~`, `*`, and `**/` for recursive search) |
| `targets.paths` | String or array | * | Alias for `path`; use one or the other, not both |
| `targets.handler` | String | * | Name of a built-in handler. Mutually exclusive with `path`/`paths` |
| `targets.description` | String | No | What this specific target contains |
| `targets.restore` | String | No | How contents are restored: `"auto"` (default) or `"manual"` |
| `targets.restore_steps` | String or array | No | Human-readable recovery instructions (e.g. `"Run npm install"`); an array becomes one line per entry |
| `targets.risk` | String | No | Impact of deletion: `"safe"` (default), `"low"`, `"medium"`, `"high"` |
| `targets.ignore` | String or array | No | Glob patterns for files/directories to preserve within this target |

\* Each target needs exactly one of `path`, `paths`, or `handler`.

Modules with a `platforms` list that doesn't include the current OS are silently skipped.

## Handlers

Some things cannot be cleaned up correctly by deleting a directory. Simulator devices and runtimes are registered with CoreSimulator: trashing `~/Library/Developer/CoreSimulator/Devices` reclaims the bytes but leaves Xcode listing devices that can no longer boot. The supported operation is `xcrun simctl delete <UDID>`, and working out *which* devices are stale requires asking `simctl` in the first place.

That knowledge is code, not data, so it lives in freespace rather than in a manifest. A handler knows how to enumerate its items, how large they are, and the exact command that removes one. A manifest selects a handler by name:

```toml
[[targets]]
handler = "xcode.simulator-devices"
description = "Simulator devices whose runtime is no longer installed"
risk = "medium"
restore = "manual"
restore_steps = "Recreate in Xcode → Window → Devices and Simulators"
```

Available handlers:

| Handler | What it manages | Removal command |
|---------|-----------------|-----------------|
| `xcode.simulator-devices` | Simulator devices whose runtime is gone | `xcrun simctl delete <UDID>` |
| `xcode.simulator-runtimes` | Installed simulator runtimes | `xcrun simctl runtime delete <UUID>` |

Naming a handler that this build of freespace doesn't have is a manifest error, caught when the module is parsed.

Two things follow from handler items not being ordinary files:

- **They cannot be trashed.** There is no undo for `simctl delete`. The cleanup screen marks them `[cannot be undone]` and shows the exact command that will run; pressing `t` (trash) asks before including them, and they always run *after* every reversible deletion so cancelling partway through leaves them untouched.
- **They are sized by their own tool**, not by walking the filesystem, so they appear with an accurate size immediately.

If a handler's tool isn't installed — no Xcode, or not macOS — its target simply yields nothing.

## Path Patterns

Target paths support two forms of expansion:

**Home directory** — `~` at the start of a path expands to the user's home directory:

```toml
path = "~/Library/Caches/MyApp"
# becomes /Users/jane/Library/Caches/MyApp
```

**Glob expansion** — `*` matches any files or directories at that level (powered by the `glob` crate):

```toml
path = "~/Library/Caches/Homebrew/*"
# matches all items inside the Homebrew cache directory
```

You can combine both:

```toml
path = "~/.cache/*/tmp"
# matches tmp/ inside any subdirectory of ~/.cache
```

Each matched path becomes a separate item in the TUI. If a glob matches nothing, it is silently ignored.

## Restore & Risk

Each target can declare how its contents are restored after deletion and the potential impact:

```toml
[[targets]]
path = "**/node_modules"
description = "Node.js dependencies"
restore = "manual"
restore_steps = "Run `npm install` in the project directory"
risk = "safe"
```

**`restore`** — How the contents come back after deletion:

| Value | Meaning |
|-------|---------|
| `auto` (default) | Rebuilt automatically by the system (e.g. caches, derived data) |
| `manual` | Requires a user action to restore (e.g. `npm install`, `pod install`) |

**`restore_steps`** — Human-readable instructions shown in the TUI when `restore = "manual"`. Tells the user exactly what to run to get things working again. Give one string, or an array of them for instructions that need more than a sentence.

**`risk`** — Potential impact of deletion:

| Value | Meaning |
|-------|---------|
| `safe` (default) | No meaningful impact — safe to remove freely |
| `low` | Minor inconvenience at most |
| `medium` | May contain user data worth reviewing |
| `high` | Likely data loss without a backup |

These fields are independent. For example, `node_modules` is `restore = "manual"` (needs a command) but `risk = "safe"` (no data loss). A downloads folder might be `restore = "auto"` but `risk = "medium"` (could contain files worth keeping).

All three fields are optional and default to `restore = "auto"`, `risk = "safe"`.

## Ignore Patterns

Targets can declare files or directories to preserve when cleaning, using the `ignore` field. This accepts a single glob or a list of globs, matched against the names of top-level entries in the target directory.

```toml
[[targets]]
path = "~/Library/Developer/CoreSimulator/Devices"
description = "iOS simulator device images"
ignore = "device_set.plist"
risk = "medium"
```

Multiple patterns:

```toml
[[targets]]
path = "~/.docker/buildx"
ignore = ["config.json", "*.lock"]
```

When `ignore` is set, freespace deletes (or trashes) the directory's children individually, skipping any that match an ignore pattern. The directory itself and ignored entries are left intact. Size calculations also exclude ignored entries.

Patterns must be relative (no leading `/`) and must not contain directory traversal (`..`). Standard glob syntax is supported (`*`, `?`, `[...]`).

## Local Targets

Local targets discover directories by name across the user's project directories using `**/` prefix notation (familiar from `.gitignore`):

```toml
[[targets]]
path = "**/node_modules"
description = "Node.js dependencies"

[[targets]]
path = "**/target"
description = "Rust build artifacts"
```

The `**/` prefix tells freespace to recursively search through configured search directories for directories matching the given name. Hidden directories are skipped, and the scanner does not recurse into matched directories.

Discovered items are displayed with project context: `my-app/node_modules` rather than just `node_modules`.

### Configuring Search Directories

Local targets produce **zero results** until the user configures where to search. There are no default search directories.

Add `search_dirs` to `~/.config/freespace/config.toml`:

```toml
search_dirs = ["~/Projects", "~/work"]
```

Or use the `--search-dir` CLI flag:

```
freespace --search-dir ~/Projects
```

Both methods can be combined; CLI flags are merged with config file entries.

The scanner skips hidden directories (except when the target itself is hidden, like `.build`) and does not recurse into matched directories.

## Complete Example

A module that scans for Node.js package manager caches:

```toml
name = "Node Package Caches"
version = "1.0.0"
description = "Cached packages from npm, Yarn, and pnpm. Safe to delete; packages are re-downloaded on next install."
author = "freespace"
platforms = ["macos", "linux"]
icon = "U+E718"
icon_color = "#539E43"

[[targets]]
path = "~/.npm/_cacache"
description = "npm download cache"
restore = "auto"
risk = "safe"

[[targets]]
path = "~/.yarn/cache"
description = "Yarn berry cache directory"
restore = "auto"
risk = "safe"

[[targets]]
path = "~/.local/share/pnpm/store"
description = "pnpm content-addressable store"
restore = "auto"
risk = "safe"
```

## Testing Locally

Place your module directory in the default modules location:

```
~/.config/freespace/modules/my-module/module.toml
```

Or point freespace at a custom directory with the `--module-dir` flag:

```
freespace --module-dir /path/to/your/modules
```

This scans the given directory for subdirectories containing `module.toml`, in addition to the default location. Run freespace and verify your module appears and its targets resolve to the expected paths.

## Distribution

Most users never need this — the built-in catalog covers the common cases. Distribute a module when you have paths specific to your setup, or want to override a built-in.

Modules are distributed as Git repositories. Freespace clones the repo and copies module directories into `~/.config/freespace/modules/`.

### Single-module repo

Place `module.toml` at the repository root:

```
my-module-repo/
  module.toml
```

Install with:

```
freespace module install github:owner/repo
```

### Multi-module repo

Each module lives in its own subdirectory:

```
my-modules-repo/
  rust-caches/
    module.toml
  go-caches/
    module.toml
```

Install all modules (interactive selection prompt):

```
freespace module install github:owner/repo
```

Install a specific module:

```
freespace module install github:owner/repo#rust-caches
```

### Pinning a version

Append `@ref` to pin to a tag, branch, or commit:

```
freespace module install github:owner/repo@v1.0.0
freespace module install github:owner/repo@main#rust-caches
```

### Managing installed modules

```
freespace module list       # every module, built-in and installed
freespace module inspect X  # show manifest and source info
freespace module remove X   # uninstall a module
freespace module prune      # reconcile installed modules with the catalog
```

## Tips

- The `description` field should mention whether targets are safe to delete (e.g. "Safe to delete; re-downloaded on next install") so users can make informed decisions in the TUI.
- Use glob patterns to catch multiple items under a directory rather than listing each one individually.
- Platform-specific paths need separate `[[targets]]` entries — there is no per-target platform filter.
- Freespace calculates the size of each matched path recursively, so targeting a broad glob like `~/*` will be slow. Be specific.
- The module directory name is used as the install identifier. Keep it short and descriptive (e.g. `docker`, `npm-cache`).
