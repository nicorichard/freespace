# Task: Mise Delivery

## Goal

Enable installation via mise using the `github` backend. This requires **zero infrastructure** beyond GitHub Releases.

## UX

```bash
# Install latest
mise use -g github:nicorichard/freespace@latest

# Install specific version
mise use -g github:nicorichard/freespace@v0.1.0
```

## How it works

The `github` backend discovers binaries from GitHub Releases by parsing archive filenames. It looks for target triples in the archive names to match the user's OS and architecture.

Our naming convention already matches what `github` expects:

```
freespace-v0.1.0-aarch64-apple-darwin.tar.gz
freespace-v0.1.0-x86_64-apple-darwin.tar.gz
```

That's it. No plugin repo, no registry entry, no configuration files needed.

## What to do

1. **Verify it works** -- once the release workflow produces archives, test the `mise use` command
2. **Document in README** -- add mise as an installation option

## Verification

- [ ] `mise use -g github:nicorichard/freespace@v0.1.0` installs successfully
- [ ] `freespace --version` outputs correct version
- [ ] Test on both Apple Silicon and Intel Macs
- [ ] `mise use -g github:nicorichard/freespace@latest` resolves to correct version

## Depends on

- `release-workflow.md` -- needs published GitHub Releases with correctly-named archives
