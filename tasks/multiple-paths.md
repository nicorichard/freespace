# Multiple Paths Per Target

## TOML Schema

Support both `path` (string) and `paths` (array) per target. Exactly one required.

```toml
[[targets]]
paths = [
  "~/Library/Developer/Xcode/DerivedData/*",
  "~/Library/Developer/Xcode/Archives",
  "~/Library/Developer/CoreSimulator/Caches",
]
description = "Xcode build artifacts and caches"
```

## Struct Changes (`manifest.rs`)

Introduce `RawTarget` with `Option<String>` path + `Option<Vec<String>>` paths. Deserialize into `RawTarget`, then convert to `Target { paths: Vec<String>, description: Option<String> }` in `Module::parse` with validation:

- `path` only → `vec![path]`
- `paths` only → use directly (must be non-empty)
- Both or neither → error

## Safety Validation (`manifest.rs`)

Loop `validate_target_pattern` over all entries in `target.paths`.

## Scanner Changes (`scanner.rs`)

Add inner loop in `scan_module`: `for path_pattern in &target.paths { ... }`. Existing branch logic (local vs global target) unchanged. `target_description` shared across all paths.

## Display (`main.rs`)

Change `target.path` print to `target.paths.join(", ")`.

## Tests

- Update all direct `Target` construction to use `paths: vec![...]`
- Add tests: multi-path parsing, both-present error, empty-paths error

## Bundled Module Candidates

- **xcode-extras**: Merge device logs targets (iOS + watchOS)
- Other modules mostly fine as-is

## Sequence

1. `manifest.rs`: RawTarget + Target refactor + validation
2. `manifest.rs`: Safety validation loop
3. `scanner.rs`: Inner loop over target.paths + fix tests
4. `main.rs`: Display update
5. New tests for multi-path parsing/errors
6. Optionally consolidate xcode modules
