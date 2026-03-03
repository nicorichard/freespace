# Next Steps

Track progress by replacing `[ ]` with `[x]` as each task is completed.

## Execution Order

- [x] 1. [Header Improvement](tasks/header-improvement.md) — Change "total" to "reclaimable"
- [x] 2. [Keybindings Helper](tasks/keybindings-helper.md) — Redesign bottom bar with `[key] action` style, shared widget
- [x] 3. [Version Display](tasks/version-display.md) — Show `v0.1.0` in bottom-right of status bar
- [x] 4. [Module ID](tasks/module-id.md) — Add `id` field to manifests, unify CLI identification
- [ ] 5. [System Free Space](tasks/system-free-space.md) — Show disk free/total in header via `statvfs`
- [ ] 6. [Info Pane](tasks/info-pane.md) — `[i]nfo` overlay with module metadata and actions
- [ ] 7. [Install Selection UX](tasks/install-selection-ux.md) — Replace dialoguer with ratatui multi-select
- [ ] 8. [Security Audit](tasks/security-audit.md) — Path validation, deny-lists, symlink safety, audit trail

## Dependency Graph

```
1. header-improvement ──► 5. system-free-space
2. keybindings-helper ─┬► 3. version-display
                       └► 7. install-selection-ux
4. module-id ──────────┬► 6. info-pane
                       └► 7. install-selection-ux
8. security-audit (independent — gate before public release)
```
