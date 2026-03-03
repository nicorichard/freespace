# Task: Header Improvement — "total" to "reclaimable"

## Goal

Change the header text from `Freespace — 56.6 GB total` to `Freespace — 56.6 GB reclaimable` so users immediately understand the number represents space they can reclaim.

## Why

"total" is ambiguous — it could mean total disk, total scanned, or total reclaimable. "reclaimable" communicates the value proposition: this is how much space you can free up.

## How

In `src/tui/views/module_list.rs` line ~145, change:

```rust
format!(" Freespace \u{2014} {} total ", format_size(total))
```

to:

```rust
format!(" Freespace \u{2014} {} reclaimable ", format_size(total))
```

## UX

Before:
```
 Freespace — 56.6 GB total
```

After:
```
 Freespace — 56.6 GB reclaimable
```

## Verification

- [ ] Header reads "reclaimable" instead of "total"
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

Nothing — standalone change.

## Blocks

- `system-free-space.md` — header area will be extended with disk stats
