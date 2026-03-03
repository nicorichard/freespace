# Task: System Free Space Display

## Goal

Show the system's disk free space and total capacity in the header bar alongside the reclaimable total.

## Why

Users want context: "56 GB reclaimable" is more meaningful when they can see "out of 500 GB total, 120 GB free." It helps prioritize cleanup urgency.

## How

### 1. Disk stats via `statvfs`

On Unix systems, use `libc::statvfs` to query the root filesystem (`/`):

```rust
#[cfg(unix)]
fn disk_stats() -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    let path = CString::new("/").ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let ret = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };
    if ret == 0 {
        let stat = unsafe { stat.assume_init() };
        let total = stat.f_blocks * stat.f_frsize;
        let free = stat.f_bavail * stat.f_frsize;  // available to non-root
        Some((total, free))
    } else {
        None
    }
}
```

Add `libc` as a dependency in `Cargo.toml` (unix-only).

### 2. Non-Unix fallback

On non-Unix platforms, return `None` and skip the display. This is acceptable since the primary target is macOS.

### 3. Header integration

Update `render_title_bar` in `src/tui/views/module_list.rs` to include disk stats:

```
 Freespace — 56.6 GB reclaimable │ 120 GB free / 500 GB
```

Or when disk stats are unavailable:
```
 Freespace — 56.6 GB reclaimable
```

### 4. Refresh strategy

Query disk stats once at startup and store in `App` state. Optionally refresh after cleanup operations complete (since deleting files changes free space).

## UX

```
 Freespace — 56.6 GB reclaimable │ 120.3 GB free / 494.4 GB
```

After cleaning 10 GB:
```
 Freespace — 46.6 GB reclaimable │ 130.3 GB free / 494.4 GB
```

## Verification

- [ ] Header shows disk free/total on macOS
- [ ] Sizes use the existing `format_size()` helper
- [ ] Graceful fallback when `statvfs` is unavailable
- [ ] Disk stats refresh after cleanup operations
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

- `header-improvement.md` — the header text is updated first (total → reclaimable)

## Blocks

Nothing.
