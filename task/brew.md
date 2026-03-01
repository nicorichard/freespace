# Task: Homebrew Delivery

## Goal

Distribute freespace via Homebrew using a custom tap with pre-built binaries.

## Why a custom tap

- `homebrew-core` requires project notability (stars, usage metrics)
- A custom tap (`nicorichard/homebrew-freespace`) gives full control
- Pre-built binaries mean users don't need a Rust toolchain

## UX

```bash
brew install nicorichard/freespace/freespace
```

## Setup

### 1. Create the tap repo

Create `github.com/nicorichard/homebrew-freespace` with a `Formula/` directory.

### 2. Formula: `Formula/freespace.rb`

```ruby
class Freespace < Formula
  desc "Interactive terminal interface for browsing and cleaning disk space consumers"
  homepage "https://github.com/nicorichard/freespace"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/nicorichard/freespace/releases/download/v#{version}/freespace-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_ARM64_SHA256"
    else
      url "https://github.com/nicorichard/freespace/releases/download/v#{version}/freespace-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_X86_64_SHA256"
    end
  end

  def install
    bin.install "freespace"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/freespace --version")
  end
end
```

Note: `src/main.rs:13` has `#[command(version)]` which enables `--version` for the Homebrew test block.

### 3. Auto-update workflow

Add `.github/workflows/update-homebrew.yml` to the **freespace** repo (not the tap repo):

```yaml
name: Update Homebrew Tap
on:
  release:
    types: [published]
```

Steps:
1. Download the release assets (archives + checksums)
2. Extract SHA-256 hashes from the `.sha256` files
3. Clone the tap repo using `HOMEBREW_TAP_TOKEN`
4. Update `Formula/freespace.rb` with new version and SHA-256 values
5. Commit and push to the tap repo

### 4. Required secret

Add a GitHub PAT as `HOMEBREW_TAP_TOKEN` in the freespace repo settings. The PAT needs `repo` scope for push access to `nicorichard/homebrew-freespace`.

## Verification

- [ ] `brew tap nicorichard/freespace`
- [ ] `brew install nicorichard/freespace/freespace`
- [ ] `freespace --version` outputs correct version
- [ ] `brew test freespace` passes
- [ ] Test on both Apple Silicon and Intel Macs
- [ ] Push a new release and verify the tap auto-updates

## Depends on

- `release-workflow.md` -- needs published GitHub Releases with archives + checksums
