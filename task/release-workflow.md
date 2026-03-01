# Task: GitHub Release Workflow

> Shared prerequisite -- all delivery methods depend on this.

## Goal

Create a GitHub Actions workflow that builds universal macOS binaries and publishes them as GitHub Releases on version tags.

## Constraint

The `trash` crate links macOS system frameworks (`objc2`), so macOS binaries **cannot be cross-compiled from Linux**. Both arch builds must run on native macOS CI runners.

## Workflow: `.github/workflows/release.yml`

### Trigger

```yaml
on:
  push:
    tags: ['v*']
```

### Jobs

**1. `build` (matrix)**

| Runner       | Target                    |
|-------------|---------------------------|
| `macos-14`  | `aarch64-apple-darwin`    |
| `macos-13`  | `x86_64-apple-darwin`     |

Steps:
1. Checkout
2. Install Rust toolchain (stable)
3. **Version sync check** -- extract version from `Cargo.toml`, compare to tag (strip `v` prefix). Fail if mismatch.
4. `cargo build --release`
5. Create archive: `freespace-v{version}-{target}.tar.gz` containing the `freespace` binary
6. Generate SHA-256 checksum: `freespace-v{version}-{target}.tar.gz.sha256`
7. Upload archives + checksums as job artifacts

**2. `release` (needs: build)**

Steps:
1. Download all artifacts
2. Create GitHub Release from the tag using `gh release create`
3. Attach all `.tar.gz` and `.sha256` files to the release
4. Auto-generate release notes from commits

### Archive naming convention

```
freespace-v0.1.0-aarch64-apple-darwin.tar.gz
freespace-v0.1.0-aarch64-apple-darwin.tar.gz.sha256
freespace-v0.1.0-x86_64-apple-darwin.tar.gz
freespace-v0.1.0-x86_64-apple-darwin.tar.gz.sha256
```

This naming convention uses target triples, which is important for `ubi` backend compatibility (see `mise.md`).

### SHA-256 checksum format

```
<hash>  freespace-v0.1.0-aarch64-apple-darwin.tar.gz
```

Standard `shasum -a 256` output format (hash, two spaces, filename).

## Version sync

`Cargo.toml` version (currently `0.1.0`) must match the git tag. The workflow enforces this:

```bash
CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
TAG_VERSION="${GITHUB_REF_NAME#v}"
if [ "$CARGO_VERSION" != "$TAG_VERSION" ]; then
  echo "Version mismatch: Cargo.toml=$CARGO_VERSION tag=$TAG_VERSION"
  exit 1
fi
```

## Release process

```bash
# 1. Bump version in Cargo.toml
# 2. Commit
git add Cargo.toml Cargo.lock
git commit -m "bump version to 0.2.0"
# 3. Tag and push
git tag v0.2.0
git push origin main --tags
```

## Verification

- [ ] Push a `v0.1.0` tag
- [ ] Both matrix jobs complete successfully
- [ ] GitHub Release appears with 4 attachments (2 archives + 2 checksums)
- [ ] Download each archive, extract, and run `./freespace --version`
- [ ] Checksums match: `shasum -a 256 -c freespace-v0.1.0-*.sha256`

## Depends on

Nothing -- this is the first task.

## Blocks

- `brew.md`
- `mise.md`
- `curl.md`
