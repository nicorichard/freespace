# Pre-check file permissions before trash/delete

## Problem

When a user selects items for cleanup that are outside user space (or otherwise
not writable), the operation silently fails at execution time. The user goes
through the full selection + confirmation flow, presses `t` or `d`, and only
then discovers the failure in the results. There's no upfront warning.

## Where it happens

- `cleaner.rs:trash_items` / `cleaner.rs:delete_items` — failures are captured
  in `CleanupResult.failed` but only after the attempt.
- `cleanup_confirm.rs` — the confirmation view shows safety-level warnings
  (`[!]` for Warn-tier paths) but has no concept of write permissions.

## Root cause

The safety system (`safety.rs`) classifies paths by location (system paths,
sensitive dirs, outside-home) but never checks OS-level write permissions.
Trashing/deleting requires write access to the **parent directory** (to unlink)
and sometimes the item itself. Items in `/Library`, `/opt`, or other
root-owned locations will always fail without elevated permissions.

## Proposed solution

### 1. Add a writability check in `safety.rs`

```rust
/// Check if the current user can likely write/delete this path.
/// Checks write permission on the parent directory (needed to unlink).
pub fn check_writable(path: &Path) -> bool {
    let dir = path.parent().unwrap_or(path);
    // On Unix: check write bit via metadata or access(2)
    // On Windows: try a test open or check ACLs
    std::fs::metadata(dir)
        .map(|m| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let uid = unsafe { libc::getuid() };
                if uid == 0 { return true; } // root can write anywhere
                let mode = m.mode();
                if m.uid() == uid { mode & 0o200 != 0 }
                else { mode & 0o002 != 0 } // simplified; doesn't check group
            }
            #[cfg(not(unix))]
            { !m.permissions().readonly() }
        })
        .unwrap_or(false)
}
```

Alternatively, use `libc::access(path, W_OK)` which also accounts for ACLs
and group membership — simpler and more correct on macOS/Linux.

### 2. Surface in the confirmation view (`cleanup_confirm.rs`)

- In `collect_selected_items`, run the writability check for each item.
- Add a visual indicator alongside existing safety markers:
  ```
  Docker.raw    .../Docker/Docker.raw    42.3 GB  [no write access]
  ```
- Add a summary line:
  ```
  3 items — 42.3 GB to reclaim  [!] 1 item may require elevated permissions
  ```

### 3. Optionally: surface in module detail view too

Flag items as read-only when first discovered during scanning, so the user
sees it before even selecting. This is more work (needs a field on `Item`)
but gives earlier feedback.

## Design considerations

- **Don't block, just warn.** The permission check is a heuristic — ACLs,
  group membership, and macOS sandbox entitlements can make it wrong. Let the
  user attempt the operation regardless.
- **`access(2)` vs metadata check.** `access(2)` is more accurate because it
  checks effective permissions including groups and ACLs. Metadata bit-checking
  is simpler but misses group membership.
- **Performance.** One `access()` syscall per item is negligible compared to
  the size-scanning work already done.
- **Cross-platform.** Unix: `libc::access` or `nix::unistd::access`.
  Windows: `std::fs::metadata().permissions().readonly()` as a rough proxy.
