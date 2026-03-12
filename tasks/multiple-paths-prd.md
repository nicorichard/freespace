# PRD: Multiple Paths Per Target

## Introduction

Currently, each target in a freespace module manifest supports only a single `path` field. This means related paths (e.g., multiple Xcode cache directories) must be split into separate targets, leading to cluttered module manifests and fragmented size reporting. This feature extends the TOML schema to support a `paths` array field on targets, allowing multiple related glob patterns to be grouped under a single target with a shared description and aggregated size.

The existing `path` field remains supported permanently as a convenience alias for a single-element `paths` array.

## Goals

- Allow module authors to group related paths under a single target using a `paths` array
- Maintain full backward compatibility with the existing `path` field
- Aggregate sizes from all paths in a target into a single total
- Validate all paths in the array against existing safety rules
- Keep the scanner, display, and CLI inspect output working correctly with the new structure

## User Stories

### US-001: Introduce RawTarget deserialization struct
**Description:** As a developer, I need an intermediate `RawTarget` struct that deserializes from TOML with both optional `path` and `paths` fields, so that I can validate and normalize them into the canonical `Target` struct.

**Acceptance Criteria:**
- [ ] Add `RawTarget` struct in `src/module/manifest.rs` with `path: Option<String>` and `paths: Option<Vec<String>>` and `description: Option<String>`
- [ ] Add `#[derive(Deserialize)]` on `RawTarget`
- [ ] `Module` deserialization uses `RawTarget` internally (add a `RawModule` or use `#[serde(try_from)]` / manual conversion)
- [ ] `cargo check` passes with no warnings

### US-002: Convert RawTarget to Target with validation
**Description:** As a developer, I need `RawTarget` to be converted into `Target { paths: Vec<String>, description: Option<String> }` with validation rules, so that modules always have a well-formed target at runtime.

**Acceptance Criteria:**
- [ ] `Target` struct field changes from `path: String` to `paths: Vec<String>`
- [ ] Conversion logic in `Module::parse` (or a `RawTarget::into_target` method):
  - `path` only: converts to `paths: vec![path]`
  - `paths` only: used directly (must be non-empty)
  - Both `path` and `paths` present: returns an error with a clear message
  - Neither `path` nor `paths` present: returns an error with a clear message
  - `paths` is an empty array: returns an error with a clear message
- [ ] `cargo check` passes with no warnings

### US-003: Update safety validation for multiple paths
**Description:** As a developer, I need `Module::parse` to validate every path in `target.paths` against `safety::validate_target_pattern`, so that directory traversal attacks are blocked for all paths in a multi-path target.

**Acceptance Criteria:**
- [ ] The validation loop in `Module::parse` iterates over `target.paths` and calls `validate_target_pattern` on each entry
- [ ] If any path in the array fails validation, the entire module parse fails with an error
- [ ] Existing test `parse_rejects_traversal_in_target` still passes
- [ ] `cargo check` passes with no warnings

### US-004: Update scanner to iterate over multiple paths per target
**Description:** As a developer, I need `scan_module` in `src/core/scanner.rs` to iterate over all paths in `target.paths` instead of using a single `target.path`, so that items from all paths are discovered and sized.

**Acceptance Criteria:**
- [ ] `scan_module` has an inner loop: `for path_pattern in &target.paths { ... }`
- [ ] The existing local target (`**/dirname`) vs global target branching logic is unchanged, just applied per path
- [ ] `target_description` is shared across all paths within the target
- [ ] Items from all paths in a target contribute to the same module's item list (flat, no grouping)
- [ ] The `start_scan_sends_messages` test in `scanner.rs` is updated to use `paths: vec![...]` and passes
- [ ] `cargo test` passes

### US-005: Update CLI inspect display for multiple paths
**Description:** As a user, I want `freespace module inspect` to show all paths for a target, so I can see what a multi-path target covers.

**Acceptance Criteria:**
- [ ] In `cmd_inspect` in `src/main.rs`, the target display line changes from `target.path` to `target.paths.join(", ")`
- [ ] Single-path targets display identically to before (just one path, no trailing comma)
- [ ] Multi-path targets show comma-separated paths
- [ ] `cargo check` passes with no warnings

### US-006: Update all existing Target construction sites
**Description:** As a developer, I need to update every place that constructs a `Target` directly (tests, scanner test, etc.) to use the new `paths: Vec<String>` field, so the codebase compiles.

**Acceptance Criteria:**
- [ ] All `Target { path: ... }` usages changed to `Target { paths: vec![...] }`
- [ ] All `target.path` field accesses changed to use `target.paths` (iteration or join)
- [ ] `cargo build` succeeds with no errors or warnings
- [ ] `cargo test` passes

### US-007: Add unit tests for multi-path parsing and error cases
**Description:** As a developer, I need comprehensive tests for the new multi-path parsing behavior, so that edge cases are covered and regressions are caught.

**Acceptance Criteria:**
- [ ] Test: TOML with `paths = [...]` array parses correctly into `Target.paths`
- [ ] Test: TOML with single `path = "..."` still parses correctly (backward compat)
- [ ] Test: TOML with both `path` and `paths` on same target returns an error
- [ ] Test: TOML with neither `path` nor `paths` on a target returns an error
- [ ] Test: TOML with `paths = []` (empty array) returns an error
- [ ] Test: TOML with `paths` containing a traversal pattern (`..`) returns an error
- [ ] Test: TOML with multiple valid paths in `paths` array, all are present in parsed `Target.paths`
- [ ] All tests pass with `cargo test`

## Functional Requirements

- FR-1: The `Target` struct must store `paths: Vec<String>` instead of `path: String`
- FR-2: TOML deserialization must accept either `path` (string) or `paths` (array of strings) per target, but not both and not neither
- FR-3: An empty `paths` array must be rejected during parsing with a descriptive error
- FR-4: Every entry in `target.paths` must pass `safety::validate_target_pattern`; if any entry fails, the entire target (and module) is rejected
- FR-5: The scanner must iterate over all entries in `target.paths`, applying the existing local (`**/`) vs global path logic to each entry independently
- FR-6: All items discovered from a multi-path target share the same `target_description`
- FR-7: The `freespace module inspect` command must display all paths for a target, comma-separated
- FR-8: The existing `path` field must remain supported permanently as a convenience alias

## Non-Goals

- No bundled module consolidation in this feature (e.g., merging xcode-extras targets is a separate follow-up)
- No per-path size breakdown in the TUI -- sizes are aggregated at the target level
- No warning or deprecation of the `path` field; it is a permanent alias
- No changes to the TUI views (module list, module detail) beyond what is needed for compilation
- No new CLI flags or subcommands

## Technical Considerations

- **Serde approach:** The cleanest approach is to introduce a `RawModule` (or `RawTarget`) struct for deserialization, then convert to the canonical `Module`/`Target` in `Module::parse`. This avoids custom deserializers and keeps validation in one place.
- **Field rename:** Changing `Target.path` to `Target.paths` is a breaking internal change that will cause compiler errors at every usage site. This is intentional -- the compiler will guide you to every spot that needs updating. Address all sites in US-006.
- **Scanner inner loop:** The scanner change (US-004) is a straightforward wrapping of the existing `for target in &module.targets` body in an additional `for path_pattern in &target.paths` loop. The `item_index` counter must continue incrementing across all paths within a target.
- **Key files:**
  - `/Users/nico/Projects/freespace/src/module/manifest.rs` -- Target struct, RawTarget, Module::parse, validation
  - `/Users/nico/Projects/freespace/src/core/scanner.rs` -- scan_module function
  - `/Users/nico/Projects/freespace/src/main.rs` -- cmd_inspect display

## Success Metrics

- All existing tests continue to pass (backward compatibility)
- New unit tests cover all multi-path parsing edge cases (both/neither/empty errors)
- A module TOML using `paths = [...]` is correctly parsed, validated, scanned, and displayed
- `cargo clippy` reports no new warnings
- `cargo fmt` shows no formatting changes needed

## Open Questions

- None -- all clarifying questions have been resolved.

## Implementation Order

The recommended implementation order follows dependency chains:

1. **US-001** + **US-002** (RawTarget struct + conversion logic) -- foundation
2. **US-003** (safety validation loop) -- builds on new `paths` field
3. **US-006** (update all construction sites) -- makes the codebase compile
4. **US-004** (scanner inner loop) -- functional change
5. **US-005** (CLI inspect display) -- cosmetic change
6. **US-007** (new tests) -- validation

Stories 1-3 can be done in a single focused session. Stories 4-6 can follow in a second session.
