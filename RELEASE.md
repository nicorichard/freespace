# Releasing Freespace

1. Update the version in `Cargo.toml`
2. Run `cargo build` to update `Cargo.lock`
3. Commit: `git commit -am "Bump version to X.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push && git push --tags`

The release workflow will automatically:
- Build the macOS arm64 binary
- Create a GitHub release with the binary and checksums
- Update the Homebrew formula in [nicorichard/homebrew-tap](https://github.com/nicorichard/homebrew-tap)

## Requirements

- The `HOMEBREW_TAP_TOKEN` secret must be set in the repo (a PAT with `repo` scope for the tap repo)
- The version in `Cargo.toml` must match the tag (enforced by CI)
