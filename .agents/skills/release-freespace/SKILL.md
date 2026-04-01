---
name: release-freespace
description: Release a new version of freespace following RELEASE.md. Use when the user mentions releasing, bumping version, or cutting a release.
disable-model-invocation: false
user-invocable: true
argument-hint: [patch|minor|major]
allowed-tools: Read, Edit, Bash(cargo *), Bash(git *)
---

Release freespace by following the release process in RELEASE.md.

## Steps

1. Read the current version from `Cargo.toml`
2. Determine the new version based on `$ARGUMENTS` (default: `patch`):
   - `patch`: bump the patch version (e.g. 0.0.6 -> 0.0.7)
   - `minor`: bump the minor version (e.g. 0.0.6 -> 0.1.0)
   - `major`: bump the major version (e.g. 0.0.6 -> 1.0.0)
3. Update the `version` field in `Cargo.toml`
4. Run `cargo build` to update `Cargo.lock`
5. Commit: `git commit -am "Bump version to X.Y.Z"`
6. Tag: `git tag vX.Y.Z`
7. Push: `git push && git push --tags`

Confirm the version bump with the user before making changes.
