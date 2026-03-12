# PRD: Module Tags

## Introduction

Add optional tags to module manifests so users can categorize and filter modules in the freespace TUI. Tags are simple string arrays defined in TOML manifests (e.g., `tags = ["cache", "ios"]`). Users can filter by tag using a `#`-prefixed query in the existing filter UI, and tags are displayed as dimmed badges in the module list description pane.

## Goals

- Allow module authors to categorize modules with arbitrary string tags
- Let users filter the module list (and flat view) by tag using `#tag` syntax
- Display tags visually in the description pane so users understand what a module relates to
- Tag all bundled modules with a sensible vocabulary for day-one usefulness

## User Stories

### US-001: Add tags field to Module struct

**Description:** As a developer, I need the `Module` struct to support an optional `tags` field so that tag data can be parsed from TOML manifests.

**Acceptance Criteria:**
- [ ] `Module` struct in `src/module/manifest.rs` has a new field: `pub tags: Vec<String>` with `#[serde(default)]`
- [ ] Existing manifests without `tags` parse successfully (field defaults to empty vec)
- [ ] Manifests with `tags = ["cache", "ios"]` parse the tags correctly
- [ ] All existing tests in `manifest.rs` continue to pass without modification (since `#[serde(default)]` handles missing field)
- [ ] `cargo check` passes without warnings

### US-002: Fix test compilation across codebase

**Description:** As a developer, I need all test code that constructs `Module` literals directly (not via TOML parsing) to include the new `tags` field so the project compiles.

**Acceptance Criteria:**
- [ ] Search all files for direct `Module { ... }` struct literals in test code
- [ ] Add `tags: vec![]` to each literal (files likely include `src/module/installer.rs` tests and any other test helpers)
- [ ] `cargo test` compiles and all existing tests pass
- [ ] `cargo clippy` passes

### US-003: Add tag parsing tests

**Description:** As a developer, I want unit tests that verify tag parsing from TOML, including edge cases.

**Acceptance Criteria:**
- [ ] New test in `manifest.rs`: parsing a manifest with `tags = ["cache", "build-artifacts"]` produces correct `Vec<String>`
- [ ] New test: parsing a manifest with `tags = []` produces empty vec
- [ ] New test: parsing a manifest without the `tags` field produces empty vec (backward compatibility)
- [ ] All tests pass with `cargo test`

### US-004: Update matches_filter to support tag search

**Description:** As a user, I want to type `#cache` in the filter bar to see only modules tagged with "cache", so I can quickly find relevant modules.

**Acceptance Criteria:**
- [ ] `matches_filter` signature changes to: `pub fn matches_filter(haystack: &str, tags: &[String], query: &str) -> bool`
- [ ] When query starts with `#`, the remainder is matched against tags only (case-insensitive substring match against each tag)
- [ ] When query does NOT start with `#`, behavior is unchanged (matches against haystack), but ALSO matches against tags as a fallback
- [ ] When query is empty, returns `true` (unchanged)
- [ ] When query is exactly `#` (no tag name), returns `true` (treat as empty filter)
- [ ] `cargo check` passes without warnings

### US-005: Update matches_filter call sites

**Description:** As a developer, I need to pass tags to all existing `matches_filter` call sites so tag filtering works across all views.

**Acceptance Criteria:**
- [ ] `src/tui/views/module_list.rs` — `sorted_module_indices` and `all_sorted_module_indices` pass `&module.tags` to `matches_filter`
- [ ] `src/tui/views/flat_view.rs` — the flat view item filter passes module tags to `matches_filter`
- [ ] `src/tui/views/module_detail.rs` — `sorted_item_indices` passes empty tags `&[]` (items don't have tags, only modules do)
- [ ] `src/tui/views/cleanup_confirm.rs` — passes empty tags `&[]` (item-level filtering, no tags)
- [ ] All views filter correctly when a `#`-prefixed query is active
- [ ] `cargo check` passes without warnings

### US-006: Update matches_filter tests

**Description:** As a developer, I want unit tests covering the new tag filtering behavior.

**Acceptance Criteria:**
- [ ] Existing `matches_filter` tests updated to pass empty tags (preserving current behavior)
- [ ] New test: `#cache` matches a module with tags `["cache", "ios"]`
- [ ] New test: `#cache` does NOT match a module with tags `["ios", "build-artifacts"]`
- [ ] New test: `#` alone (empty tag query) matches everything
- [ ] New test: `#CA` matches tag `"cache"` (case-insensitive)
- [ ] New test: plain query `cache` matches against both haystack and tags
- [ ] All tests pass with `cargo test`

### US-007: Display tag badges in description pane

**Description:** As a user, I want to see a module's tags displayed as dimmed badges in the description pane so I know what categories a module belongs to.

**Acceptance Criteria:**
- [ ] `render_description_pane` in `src/tui/views/module_list.rs` appends tag badges after the description text
- [ ] Tags render in the format: `  [cache] [ios]` using dimmed style from the theme
- [ ] When a module has no tags, nothing extra is appended (just the description as before)
- [ ] Tags are separated by a single space
- [ ] `cargo check` passes without warnings

### US-008: Add tags to bundled module manifests

**Description:** As a user, I want the bundled modules to come pre-tagged so filtering by tag is useful out of the box.

**Acceptance Criteria:**
- [ ] All TOML module manifests under the default modules directory (`~/.config/freespace/modules/`) and test fixtures are updated with appropriate tags
- [ ] Tag vocabulary used: `cache`, `build-artifacts`, `logs`, `ios`, `js`, `jvm`, `devtools`, `system`, `gaming`, `media` (and others as appropriate)
- [ ] Tag assignments follow the mapping from the plan (see table below)
- [ ] All manifests still parse successfully (`cargo test` passes)

Tag mapping reference:
| Module | Tags |
|--------|------|
| adobe-cache | cache, media |
| android-dev | cache, devtools |
| bun | cache, js |
| npm-cache | cache, js |
| node-modules | build-artifacts, js |
| cargo-rust | cache, devtools |
| docker | devtools |
| xcode-derived-data | build-artifacts, ios |
| xcode-archives | build-artifacts, ios |
| jetbrains-logs | logs, jvm, devtools |
| steam | cache, gaming |
| system-logs | logs, system |

### US-009: Add tags to test fixture manifests

**Description:** As a developer, I need the test fixture TOML files to include tags so that tag-related integration tests can be written.

**Acceptance Criteria:**
- [ ] `tests/fixtures/modules/single-module/module.toml` has `tags = ["cache"]` (or similar appropriate tag)
- [ ] `tests/fixtures/modules/multi-module/alpha/module.toml` and `beta/module.toml` have distinct tags for testing filter behavior
- [ ] `tests/fixtures/modules/local-target-module/module.toml` has at least one tag
- [ ] `cargo test` passes

## Functional Requirements

- FR-1: The `Module` struct must include an optional `tags: Vec<String>` field that defaults to an empty vector when absent from TOML
- FR-2: Tags are arbitrary strings — no validation against a predefined vocabulary (the vocabulary is a convention, not enforced)
- FR-3: The `matches_filter` function must accept a tags parameter and support `#`-prefixed queries for tag-only filtering
- FR-4: A query of exactly `#` (hash with no tag name) must be treated as an empty filter and match all modules
- FR-5: Non-`#` queries must match against both the module name and tags
- FR-6: Tag badges must be displayed in the description pane using dimmed style, formatted as `[tagname]`
- FR-7: Tag filtering must work in all views that currently support text filtering (module list, flat view)
- FR-8: All bundled modules must ship with appropriate tags from the predefined vocabulary

## Non-Goals

- No tag management UI (users edit TOML files directly)
- No group-by-tag section headers in the module list
- No tag autocompletion in the filter bar
- No tag-based sorting
- No enforced tag vocabulary (arbitrary strings are accepted)
- No multi-tag filter syntax (e.g., `#cache+ios`)

## Technical Considerations

- The `#[serde(default)]` attribute on the `tags` field ensures backward compatibility with existing manifests that lack a `tags` field
- The `matches_filter` signature change affects 4 call sites across the view layer — all must be updated in the same story (US-005) to keep the build green
- US-004 and US-005 should be implemented together or in immediate sequence since changing the signature breaks call sites
- The description pane in `module_list.rs` currently renders a single `Line` — tags can be appended as additional `Span` elements in the same line

## Success Metrics

- Users can type `#cache` and immediately see only cache-related modules
- All bundled modules are discoverable by at least one tag
- Zero regressions: all existing tests pass, `cargo clippy` is clean
- Tag display is unobtrusive (dimmed style) and doesn't clutter the UI

## Open Questions

- Should the `#` prefix filter be documented in the help overlay (`help.rs`)? (Recommendation: yes, add a line to the help view as a follow-up)
- Should `freespace module inspect <id>` display tags in its output? (Recommendation: yes, low effort, nice to have)
