# Module Tags

## TOML Schema

Optional `tags` field on module manifest. Defaults to empty.

```toml
id = "xcode-derived-data"
name = "Xcode Derived Data"
tags = ["ios", "build-artifacts", "xcode"]
```

## Struct Change (`manifest.rs`)

```rust
#[serde(default)]
pub tags: Vec<String>,
```

Update all test `Module` literals across codebase to include `tags: vec![]`.

## Filter Integration (`app.rs`)

Extend `matches_filter` to accept tags and search them. `#`-prefixed queries match tags only.

```rust
pub fn matches_filter(haystack: &str, tags: &[String], query: &str) -> bool
```

Update 3 call sites in `module_list.rs` / flat view.

## Tag Display (`module_list.rs`)

Append tag badges to description pane: `[cache] [ios]` in dimmed style.

## Tag Vocabulary

`cache`, `build-artifacts`, `logs`, `ios`, `js`, `jvm`, `devtools`, `system`, `gaming`, `media`

## Bundled Module Tags

| Module | Tags |
|--------|------|
| adobe-cache | cache, media |
| android-dev | cache, devtools |
| bun/npm/node | cache/build-artifacts, js |
| cargo-rust | cache, devtools |
| docker | devtools |
| xcode-* | build-artifacts/ios |
| jetbrains-logs | logs, jvm, devtools |
| steam | cache, gaming |
| system-logs | logs, system |
| (etc — see full mapping in plan) |

## Not Doing

- No tag management UI
- No group-by-tag section headers
- No tag autocompletion
- No tag-based sorting

## Sequence

1. `manifest.rs`: Add `tags` field + optional validation
2. Fix test compilation (add `tags: vec![]` everywhere)
3. `app.rs`: Update `matches_filter` + call sites
4. `module_list.rs`: Tag display in description pane
5. Bundled module TOMLs: Add tags
6. New tests for tag parsing + filter-by-tag
