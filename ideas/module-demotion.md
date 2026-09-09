# Demote modules to an optional extra

## Context

freespace now ships a built-in catalog of 52 modules embedded in the binary, and
built-in handlers for cleanup that isn't path-shaped. Installed TOML modules are
no longer how the app gets its content — they are an extension point for paths
the catalog doesn't cover.

The product has moved but its presentation hasn't. The README still opens with
"powered by community-written cleanup modules" and teaches the manifest format
in its third section. The app still creates `~/.config/freespace/modules/` on
every launch, advertising a location most users will never put anything in. And
`nicorichard/freespace-modules` is still live, still carrying the two
CoreSimulator path targets that corrupt the simulator registry.

Goal: the docs, the startup behaviour, and the upstream repo all reflect that
modules are optional — without breaking anyone who already installed them, and
without deleting anything they added themselves.

## Prerequisite: retire `nicorichard/freespace-modules`

**Order matters. An archived GitHub repo is read-only — content cannot change
without unarchiving.** So the final commit has to land first.

1. One last commit to that repo:
   - Delete the `~/Library/Developer/CoreSimulator/Devices` and
     `.../CoreSimulator/Runtime` targets from `xcode-extras/module.toml`. These
     corrupt CoreSimulator's registry. Free to include here, and it protects
     anyone who runs `module update` without upgrading the binary.
   - README: state that these modules ship inside freespace and point at the
     app. No history, per the no-old-state rule — describe where things are, not
     where they were.
2. Then archive.

Archiving is safe for the app: archived public repos still clone and answer
`git ls-remote`, so existing installs and update checks keep working. The
`CATALOG_SOURCE_REPO` constant stays — it identifies duplicates by matching
strings already written into `source.toml` on disk, and never touches the
network.

## Phase 1 — Upgrade path: decouple cleanly, keep everything else

`freespace module prune` (`cmd_prune`, `src/main.rs`) today deletes an installed
module directory when **both** hold: its `source.toml` repository is
`CATALOG_SOURCE_REPO`, and its id is in the catalog. Everything else is left
alone. That is correct as far as it goes, but leaves one case dangling.

Extend it to three outcomes:

| Installed module | Action |
|---|---|
| From `CATALOG_SOURCE_REPO`, id **is** in the catalog | Delete the directory — the built-in supersedes it |
| From `CATALOG_SOURCE_REPO`, id **is not** in the catalog | Keep the module, delete only its `source.toml` |
| Any other source, or no `source.toml` | Untouched |

The middle row is the "cleanly decouples but leaves anything else they added"
case: a module forked into that repo or added to a local clone. Deleting it
would lose the user's work; leaving it intact means it keeps running update
checks against an archived remote forever. Dropping `source.toml` converts it
into a plain local module — `read_update_check_info` then returns `None` and
`spawn_update_checks` marks it `Skipped`, so the dead remote is never contacted.

Report the two outcomes separately, in present tense:

```
Removed 51 module(s) that duplicate a built-in.
Detached 1 module from its source; it is now local: my-fork
```

**Keep prune manual.** The app already behaves correctly unpruned — duplicates
are ignored at load time — so this is disk housekeeping, not a correctness fix.
Deleting from someone's config directory unprompted on first launch is exactly
the kind of surprise worth avoiding.

Make it discoverable instead by fixing `cmd_list`, which currently prints the
built-in **and** the ignored installed copy as two rows with the same id and no
indication which one is live:

```
docker    Docker & Containers   1.0.0   built-in
docker    Docker & Containers   1.0.0   duplicate of built-in (ignored)
```

Files: `src/main.rs` (`cmd_prune`, `cmd_list`). Reuse
`module::catalog::catalog_ids` and `installer::read_source_info`.

## Phase 2 — Stop advertising the modules directory at startup

`manager::load_all_modules` (`src/module/manager.rs:66-75`) calls
`create_dir_all` on `~/.config/freespace/modules/` on every launch and warns if
it fails. With the catalog built in, an empty directory there is noise.

- Drop the `create_dir_all`. Treat an absent directory as the normal case: skip
  it silently rather than warning, matching how a missing config file already
  behaves.
- Keep the `create_dir_all` in `src/main.rs:220` — the `module` subcommands
  legitimately need the directory to exist before installing.
- Leave the empty-list view in `module_list.rs` as is. It is now only reachable
  by disabling the catalog and already points at `module enable` rather than
  advertising installation.

## Phase 3 — Documentation

The through-line: lead with what freespace *does*, mention modules once as an
extension point near the end, and keep `MODULES.md` as reference for people who
go looking.

**`README.md`** — the main work.
- Opening line drops "powered by community-written cleanup modules". Lead with
  the TUI and what it already knows how to clean.
- `## Why Freespace`: keep the four bullets; **Extensible** moves last and
  shortens to a single sentence.
- `## What a module looks like` (currently section 3, with a full manifest
  example): **delete from that position.** Teaching the manifest format before
  the user has run the app inverts the priority.
- `## Community Modules` + `### Create your own`: collapse into one short
  `## Extending freespace` section near the end — a two-line description,
  `freespace module install <source>`, and a link to `MODULES.md`.
- Add a short section naming what ships built-in and how to turn one off
  (`module list`, `module disable`). This is the actual value proposition and
  is currently absent.

**`MODULES.md`** — already opens by describing the catalog first, but should be
explicit in its first line that this document is for the optional case. Keep the
filename; inbound links point at it.

**`docs/configuration.md`** — `[modules]` is currently documented first, ahead of
`module_dirs`, `search_dirs`, and the safety settings. Move it below the options
a typical user actually reaches for.

**`AGENTS.md` / `CLAUDE.md`** — already accurate. No change.

## Verification

```sh
cargo fmt --check && cargo clippy --all-targets && cargo test
```

New tests:
- `cmd_prune` classification over a fixture modules directory containing all
  four cases (catalog duplicate; repo module absent from the catalog; a local
  module with no `source.toml`; a module from an unrelated repo). Assert only
  the first is deleted, the second loses `source.toml` but keeps `module.toml`,
  and the last two are byte-identical afterwards.
- A module whose `source.toml` was dropped reports `ModuleUpdateStatus::Skipped`
  and triggers no network call.

Manual:
- Fresh `HOME`: run the TUI, confirm `~/.config/freespace/modules/` is **not**
  created and the module list is fully populated.
- Reinstall `github:nicorichard/freespace-modules` into a scratch `HOME`, add a
  hand-written local module and a fake forked one, run `freespace module prune`,
  and confirm the table above holds.
- `freespace module list` distinguishes live built-ins from ignored duplicates.

## Out of scope

- Automatic pruning on upgrade.
- Removing `installer.rs` or the install TUI views — installing modules stays
  supported, just unadvertised.
- Any change to handler behaviour or the cleanup flow.
