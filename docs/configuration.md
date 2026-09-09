# Configuration

Freespace uses an optional TOML config file at `~/.config/freespace/config.toml`. All fields are optional — if the file is missing or empty, sensible defaults are used.

## Example

```toml
module_dirs = ["~/my-custom-modules"]
search_dirs = ["~/Projects", "~/Work"]
audit_log = true
protected_paths = ["~/Work/important-project", "~/Documents"]
enforce_scope = true

[modules]
builtin = true
disabled = ["steam", "spotify"]
```

## Fields

### `module_dirs`

- **Type:** list of strings
- **Default:** `[]`

Extra directories to load modules from, in addition to the default `~/.config/freespace/modules/`.

```toml
module_dirs = ["~/my-custom-modules", "/opt/shared-modules"]
```

### `search_dirs`

- **Type:** list of strings
- **Default:** `[]`

Directories to search for items.

```toml
search_dirs = ["~/Projects", "~/Work"]
```

### `audit_log`

- **Type:** boolean
- **Default:** `true`

When enabled, cleanup actions are logged for auditing purposes.

```toml
audit_log = false
```

### `protected_paths`

- **Type:** list of strings
- **Default:** `[]`

Paths that are protected from cleanup operations. Freespace will refuse to delete anything under these paths.

```toml
protected_paths = ["~/Work", "~/Documents/important"]
```

### `enforce_scope`

- **Type:** boolean
- **Default:** `true`

When enabled, modules are restricted to operating within their declared scope.

```toml
enforce_scope = true
```

### `[modules]`

Controls the built-in module catalog that ships inside the binary.

#### `modules.builtin`

- **Type:** boolean
- **Default:** `true`

When false, the entire built-in catalog is skipped and only modules installed
under `~/.config/freespace/modules/` (plus `module_dirs`) load.

#### `modules.disabled`

- **Type:** list of strings
- **Default:** `[]`

Ids of individual built-in modules to skip.

```toml
[modules]
disabled = ["steam", "spotify"]
```

The CLI edits this for you:

```sh
freespace module disable steam
freespace module enable steam
freespace module list            # built-ins marked "built-in" / "built-in (disabled)"
```

To customise a built-in rather than disable it, install a module with the same
`id` — a user module always replaces the built-in it shadows.

## Notes

- The `dry_run` mode is controlled via the `--dry-run` CLI flag, not the config file.
- The config directory is always `~/.config/freespace/`, regardless of platform.
- Modules are loaded from the built-in catalog first, then `~/.config/freespace/modules/`, then any paths listed in `module_dirs`. A user module replaces a built-in with the same `id`.
- An installed module that duplicates a built-in is ignored in favour of the built-in. `freespace module prune` clears those duplicates off disk, and detaches anything else installed from the same source so it loads as a plain local module.
