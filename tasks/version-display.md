# Task: Version Display in Status Bar

## Goal

Show the app version (e.g., `v0.1.0`) in the bottom-right of the status bar.

## Why

Users should know which version they're running without needing `--version`. It also provides a subtle branding touch and helps with bug reports.

## How

### 1. Version constant

Use the `env!("CARGO_PKG_VERSION")` macro which is already available via clap's `#[command(version)]` in `src/main.rs`. Create a formatted version string:

```rust
let version = format!("v{}", env!("CARGO_PKG_VERSION"));
```

### 2. Render in status bar

In the shared `keybinding_bar()` widget (or in each view's status bar renderer), right-align the version string in the status bar area:

- Left side: keybinding hints
- Right side: `v0.1.0` in a muted/dim style

### 3. Layout

Use a ratatui `Layout` split or `Line` with left/right alignment to position the version flush-right within the status bar `Rect`.

## UX

```
 [space] select │ [a]ll │ [n]one │ [/] filter │ [c]lean │ [?] help │ [q]uit                v0.1.0
```

## Verification

- [ ] Version displays in the bottom-right corner of every view
- [ ] Version matches `cargo run -- --version` output
- [ ] Version text uses muted/dim styling, not distracting
- [ ] `cargo clippy` passes
- [ ] `cargo test` passes

## Depends on

- `keybindings-helper.md` — the status bar redesign creates the layout where the version sits

## Blocks

Nothing.
