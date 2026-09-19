# Show only leftover IDE versions

## Context

JetBrains IDEs and Android Studio keep their state in folders named for the
IDE version: `IntelliJIdea2025.3`, `AndroidStudio2026.1.3`. There is one set in
Caches, one in Logs, and one in Application Support (on Linux: `~/.cache`,
`~/.config`, `~/.local/share`). A major update creates a fresh set and imports
settings from the previous one, but never deletes the old folders. The Caches
folder holds the indexes and is often several GB, so after a few years of
updates the old folders add up to far more than the version in use.

The `jetbrains` catalog module lists every version folder under its three
targets. That surfaces the leftovers, but the current version appears next to
them, and the user has to know which one they run. Deleting the current
version's settings folder resets the IDE's settings and plugins, which is why
that target is `risk = "medium"`.

A manifest can't express "every version except the newest": a target is a glob
plus `ignore` patterns, and neither can compare one match with another.

Goal: a target can declare that its matches are versioned folders and that only
the superseded versions should be listed. The `jetbrains` module then lists
exactly the leftovers.

## Why not a handler

Handlers exist for cleanup that isn't path-shaped. Their items are keyed by a
synthetic path, skip `check_safety`, and can't be trashed. Leftover IDE folders
are ordinary directories. They should be trashed, pass the safety checks and
get sized by the scanner like any other path item. The only missing piece is
*which* matches to list, which is a filter on a path target and not a new kind
of target.

## Manifest: `keep_newest`

```toml
[[targets]]
paths = [
    "~/Library/Caches/JetBrains/*20[0-9][0-9].[0-9]*",
    "~/Library/Caches/Google/AndroidStudio*",
]
description = "Caches and indexes of superseded IDE versions"
keep_newest = 1
```

`keep_newest = N` treats each matched name as `<product><version>` and leaves
out the newest `N` versions of each product.

- **Parsing a name.** The version is the trailing run of digits and dots and
  must start with a digit. The product is everything before it and must not be
  empty. `AndroidStudio2026.1.3` gives `("AndroidStudio", [2026, 1, 3])`, and
  `IdeaIC2024.1` gives `("IdeaIC", [2024, 1])`.
- **Ordering.** Versions compare component by component as integers (`Vec<u64>`
  ordering): `[4, 2] < [2025, 3] < [2025, 3, 1] < [2026, 1]`. That puts Android
  Studio's pre-2020 `AndroidStudio4.2` below its year-based versions with no
  special case.
- **Names with no version** are never listed by a `keep_newest` target, since
  they aren't versioned folders. This backs up the tight globs: even if a
  pattern also matched `Toolbox`, it would drop out.
- **Validation in `Module::parse`.** Reject `0`. Reject it on a handler target
  and on `**/` local patterns: local targets are per-project directories with no
  version to compare.

### The newest version is decided per module, not per target

Caches, logs and settings are separate targets because they carry different
risk levels, but which version is current is a single fact. If each target
decided on its own, two cases would go wrong:

- The user already trashed `IntelliJIdea2026.1`'s caches. In Caches the newest
  remaining folder is `IntelliJIdea2025.3`, so the Caches target would hide it,
  even though it's a leftover.
- An IDE was updated but not yet launched, so only its settings folder exists.
  Same result in the other direction.

So the scanner builds one set of versions per module from every match of every
`keep_newest` target, grouped by product. An item is listed when its version
isn't among the newest `keep_newest` versions of its product in that set. Each
item's `keep_newest` still comes from its own target.

## Changes

**`src/core/versions.rs`** (new): pure logic with no filesystem access.
- `fn parse(name: &str) -> Option<(&str, Version)>`
- `struct Installed`, built from an iterator of names, mapping each product to
  its versions sorted from newest to oldest.
- `fn superseded_by(&self, name: &str, keep: u32) -> Option<&str>` returns the
  newest folder name when `name` is a leftover, otherwise `None`. The scanner
  uses the returned name as the item's detail.

**`src/module/manifest.rs`**: add `keep_newest: Option<u32>` to `RawTarget` and
`Target`. `Target` derives `Default`, so the test fixtures don't change. Add the
validation described above.

**`src/core/scanner.rs`**: in `scan_module`, before phase 1, expand the patterns
of every `keep_newest` target once, build `Installed` from the file names, and
keep the expansions. The global-target branch then reuses those expansions and
skips any path where `superseded_by` returns `None`. Listed items get
`detail = Some(format!("newer: {newest}"))`. Nothing else changes: they stay
`CleanupAction::Path`, sized in phase 2 and trashable.

**Render `Item::detail`**: the field is populated but no view draws it, so the
simulator handlers' "last booted …" and "still selected by an installed Xcode
SDK" details are never shown either.
- `module_detail.rs`: show it dimmed after the name, or as a sub-line, whichever
  fits the row layout. Follow `docs/UI_DESIGN_GUIDE.md`, with colours through
  `Theme`.
- `cleanup_confirm.rs`: add it as a sub-row in **both** `render_items_list` and
  `visual_row_to_item_index`, in the same order (see Codebase Patterns).

**`catalog/jetbrains/module.toml`**: add `keep_newest = 1` to all three targets
and describe them as superseded versions. Once only leftovers are listed,
settings folders can drop from `medium` to `low`, because a newer version has
already imported them.

**`MODULES.md`**: add a `targets.keep_newest` row to the manifest reference and
a short "Versioned folders" section covering name parsing, ordering, and the
per-module rule.

## Verification

```sh
cargo fmt --check && cargo clippy --all-targets && cargo test
```

New tests:
- `versions`: parsing (`AndroidStudio2026.1.3`, `IdeaIC2024.1`,
  `AndroidStudioPreview2026.2` as its own product, `Toolbox` → `None`,
  `2025.3` with an empty product → `None`); ordering including `4.2` vs `2025.3`
  and `2025.3` vs `2025.3.1`; `keep = 1` and `keep = 2`; products never compared
  with each other.
- `manifest`: accepts `keep_newest`; rejects `0`, a handler target and a `**/`
  pattern.
- `scanner`: a temp dir with two `keep_newest` targets, where Caches holds only
  `Idea2025.3` and settings holds `Idea2025.3` and `Idea2026.1`. Assert that
  Caches lists `Idea2025.3` (the per-module rule) and settings lists only
  `Idea2025.3`, each with `detail = "newer: Idea2026.1"`.
- `cleanup_confirm`: an item with a detail renders the extra row, and a click on
  the row below it still maps to the next item.

Manual: in a scratch `HOME`, create fake version folders under
`Library/{Caches,Logs,Application Support}/{JetBrains,Google}` along with
`JetBrains/Toolbox` and `Google/Chrome`. Drive the TUI through a pty and confirm
that only superseded versions are listed, each shows its newer version, and
Toolbox and Chrome never appear.

## Open questions

- **Hide the newest version, or show it unselectable with a "current" tag?**
  Hiding keeps the module to exactly what can be cleaned and matches how the
  simulator handlers only list stale items. Recommended.
- **Two versions of one product side by side** (an EAP next to the stable
  release, both `IntelliJIdea*`): with `keep_newest = 1` the stable release's
  folders are listed while it's still in use. The `newer: …` detail makes that
  visible. A user who does this regularly can override the module with
  `keep_newest = 2`.
- **An IDE that was uninstalled entirely**: its newest folders are never listed,
  because nothing supersedes them. Detecting installed IDEs (`product-info.json`
  inside each `.app`) is knowledge that belongs in a handler. Leave it as a
  follow-up.

## Out of scope

- Toolbox rollback copies under `JetBrains/Toolbox/apps`. Toolbox has its own
  setting for how many previous versions to keep.
- Other versioned-folder layouts (Unity editor versions, Xcode DeviceSupport,
  language toolchains). `keep_newest` is generic, so adopting it in other
  modules is a separate change once this one lands.
