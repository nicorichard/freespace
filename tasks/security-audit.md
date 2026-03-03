# Task: Security Audit

## Goal

Audit and harden freespace against path traversal, symlink attacks, unsafe glob expansion, and accidental data loss. Establish safety invariants for a tool that deletes user files.

## Why

freespace performs destructive filesystem operations (trash and permanent delete) driven by user-supplied TOML manifests that can be installed from git repos. A malicious or careless manifest could target sensitive paths. The current codebase has minimal validation — this task adds defense-in-depth.

## How

### 1. Path validation — deny-list

Create a deny-list of paths that should never be targeted for deletion:

```rust
const DENY_PATHS: &[&str] = &[
    "/",
    "/System",
    "/usr",
    "/bin",
    "/sbin",
    "/etc",
    "/var",
    "/Applications",
    "/Library",
    // home directory protection
    "~/Documents",
    "~/Desktop",
    "~/Pictures",
    "~/Music",
    "~/Movies",
    "~/.ssh",
    "~/.gnupg",
];
```

Check resolved paths against this list before any delete/trash operation in `src/core/cleaner.rs`. Reject and warn if a path matches or is a parent of a deny-listed path.

### 2. Path validation — scope enforcement

Ensure that resolved glob paths stay within expected boundaries:

- Glob patterns from manifests should not resolve to paths outside `$HOME`
- Add a configurable `allowed_roots` (default: `["~"]`) that constrains where scanning and deletion can operate
- Validate at scan time in `src/core/scanner.rs` before processing items

### 3. Symlink safety

The scanner already uses `follow_links(false)` in `walkdir` calls, which is good. Additional hardening:

- Before deleting a directory, check if it's a symlink (don't follow into deletion)
- Resolve symlinks and re-check against deny-list
- Log warnings when symlinks are encountered in scan targets

### 4. Manifest validation

When loading manifests in `src/module/manifest.rs`:

- Reject glob patterns containing `..` path components
- Reject absolute paths outside `$HOME` (unless explicitly allowed in config)
- Warn on overly broad patterns (e.g., `~/*`, `~/*/`)

### 5. Audit trail

Add optional logging of destructive operations:

- Log each trash/delete operation with timestamp, path, size, and module source
- Write to `~/.config/freespace/audit.log`
- Enabled by default, configurable in `config.toml`

### 6. Dry-run mode

Add a `--dry-run` CLI flag that performs scanning and shows what would be cleaned without actually deleting anything. This helps users verify before committing.

## UX

Blocked operation:
```
⚠ Refusing to delete /usr/local/bin — path is in deny-list
```

Audit log entry:
```
2026-03-03T14:22:01Z TRASH ~/Library/Caches/docker (2.1 GB) [module: docker]
```

Dry-run:
```bash
freespace --dry-run
# Shows normal TUI but clean operations print what would happen without executing
```

## Verification

- [ ] Deny-listed paths are rejected before deletion
- [ ] Glob patterns with `..` are rejected at manifest load time
- [ ] Paths outside `$HOME` are rejected by default
- [ ] Symlinks are not followed during deletion
- [ ] Audit log is written for every trash/delete operation
- [ ] `--dry-run` flag shows cleanup plan without executing
- [ ] Existing functionality is not broken (all paths that worked before still work)
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes
- [ ] Add unit tests for deny-list checking and path validation

## Depends on

Nothing strictly, but best done after the codebase stabilizes:

- Benefits from `module-id.md` — audit log can reference module id
- Should be the final task before any public release

## Blocks

Nothing — but gates public release readiness.
