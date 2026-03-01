# Fix: Dry run mode blocks all deletions

## Context
The cleanup confirmation dialog shows a "DRY RUN" preview of items to delete, but pressing `y` to confirm never actually deletes anything. The `config.dry_run` flag defaults to `true` and is never toggled, so `perform_cleanup()` is unreachable.

## The Bug
- `src/config.rs:11` — `dry_run` defaults to `true`
- `src/app.rs:310-313` — when `dry_run == true` and user presses `y`, it just returns to the previous view without calling `perform_cleanup()`

## Fix
In `src/app.rs`, `handle_key_cleanup_confirm`: when the user presses `y`, always call `perform_cleanup()`. The confirmation dialog itself serves as the dry-run preview — the user has already reviewed what will be deleted. Remove the `dry_run` branch so pressing `y` always executes the cleanup.

```rust
KeyCode::Char('y') => {
    self.perform_cleanup();
    self.current_view = self.previous_view;
    self.selected_index = 0;
}
```

The `dry_run` flag and the "DRY RUN" banner in the confirmation UI can remain — they correctly communicate that the dialog is a preview. But confirming should proceed with deletion.

## Files to modify
- `src/app.rs` — lines 309-319: simplify the `y` handler to always call `perform_cleanup()`

## Verification
1. `cargo build` — ensure it compiles
2. Run the TUI, select some derived data items, press `c` to open the cleanup dialog
3. Verify the preview shows the correct items
4. Press `y` — confirm the files are actually deleted
5. Press `n`/`Esc` — confirm it still cancels without deleting
