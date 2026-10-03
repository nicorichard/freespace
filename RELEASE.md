# Releasing Freespace

1. Update the version in `Cargo.toml`
2. Run `cargo build` to update `Cargo.lock`
3. Commit: `git commit -am "Bump version to X.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push the branch and that one tag: `git push origin main vX.Y.Z`

The release workflow will automatically:
- Build the macOS arm64 binary
- Create a GitHub release with the binary and checksums
- Update the Homebrew formula in [nicorichard/homebrew-tap](https://github.com/nicorichard/homebrew-tap)

## Pre-releases

A version with a semver pre-release suffix — `0.1.0-rc.1`, tagged `v0.1.0-rc.1` —
follows the same steps and is published as a GitHub pre-release:

- It is not marked as the latest release, so `brew` and `mise ...@latest` stay
  on the last stable version
- The Homebrew formula is left alone

To try one, download the archive from the release page or pin the version:

```sh
mise use -g github:nicorichard/freespace@0.1.0-rc.1
```

## Dry run

Running the workflow by hand builds and packages the binary without publishing
anything:

```sh
gh workflow run Release --ref main
```

## Requirements

- The `HOMEBREW_TAP_TOKEN` secret must be set in the repo (a PAT with `repo` scope for the tap repo)
- The version in `Cargo.toml` must match the tag (enforced by CI)
