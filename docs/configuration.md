# Configuration

Freespace uses an optional TOML config file at `~/.config/freespace/config.toml`. All fields are optional — if the file is missing or empty, sensible defaults are used.

## Example

```toml
module_dirs = ["~/my-custom-modules"]
search_dirs = ["~/Projects", "~/Work"]
audit_log = true
protected_paths = ["~/Work/important-project", "~/Documents"]
enforce_scope = true
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

## Notes

- The `dry_run` mode is controlled via the `--dry-run` CLI flag, not the config file.
- The config directory is always `~/.config/freespace/`, regardless of platform.
- Modules are loaded from `~/.config/freespace/modules/` by default, plus any paths listed in `module_dirs`.
