# Writing Freespace Modules

Freespace modules are purely declarative TOML files that tell freespace which filesystem paths to scan for cleanup. There is no code execution — a module is just a directory containing a `module.toml` manifest that declares path patterns.

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
| `[[targets]]` | Array of tables | Yes | At least one target required |
| `targets.path` | String | Yes | Path pattern (supports `~`, `*`, and `**/` for recursive search) |
| `targets.description` | String | No | What this specific target contains |

Modules with a `platforms` list that doesn't include the current OS are silently skipped.

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

[[targets]]
path = "~/.npm/_cacache"
description = "npm download cache"

[[targets]]
path = "~/.yarn/cache"
description = "Yarn berry cache directory"

[[targets]]
path = "~/.local/share/pnpm/store"
description = "pnpm content-addressable store"
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
freespace module list       # list installed modules
freespace module inspect X  # show manifest and source info
freespace module remove X   # uninstall a module
```

## Tips

- The `description` field should mention whether targets are safe to delete (e.g. "Safe to delete; re-downloaded on next install") so users can make informed decisions in the TUI.
- Use glob patterns to catch multiple items under a directory rather than listing each one individually.
- Platform-specific paths need separate `[[targets]]` entries — there is no per-target platform filter.
- Freespace calculates the size of each matched path recursively, so targeting a broad glob like `~/*` will be slow. Be specific.
- The module directory name is used as the install identifier. Keep it short and descriptive (e.g. `docker`, `npm-cache`).
