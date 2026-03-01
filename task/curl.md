# Task: curl | bash Delivery

## Goal

Create an install script for one-line installation via curl pipe to bash.

## UX

```bash
curl -fsSL https://raw.githubusercontent.com/nicorichard/freespace/main/scripts/install.sh | bash
```

## Script: `scripts/install.sh`

### Behavior

1. `set -euo pipefail` -- fail on any error
2. Detect OS (macOS only for now) and architecture (`arm64` -> `aarch64`, `x86_64`)
3. Determine version: use `FREESPACE_VERSION` env var if set, otherwise fetch latest from GitHub API
4. Determine install directory: use `FREESPACE_INSTALL_DIR` if set, otherwise `~/.local/bin`
5. Create a temp directory with `mktemp -d`, register a `trap` to clean it up on exit
6. Download the archive and its `.sha256` checksum file
7. **Verify SHA-256 checksum** -- fail with clear error if mismatch
8. Extract binary from archive
9. Install binary to target directory (create directory if needed, **no sudo**)
10. Check if install directory is in `$PATH`, print setup instructions if not

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FREESPACE_VERSION` | latest release | Pin to specific version (e.g., `v0.1.0`) |
| `FREESPACE_INSTALL_DIR` | `~/.local/bin` | Override install location |

### Error handling

- Unsupported OS -> clear error message
- Unsupported architecture -> clear error message
- Download failure -> caught by `set -e`
- Checksum mismatch -> explicit error with expected vs actual hash
- No write permission to install dir -> suggest using `FREESPACE_INSTALL_DIR`

### PATH detection

If `~/.local/bin` is not in `$PATH`, print:

```
freespace was installed to ~/.local/bin/freespace

Add it to your PATH by adding this to your shell profile:
  export PATH="$HOME/.local/bin:$PATH"

Then restart your shell or run:
  export PATH="$HOME/.local/bin:$PATH"
```

### Security considerations

- No `sudo` escalation anywhere in the script
- Checksum verification before installation
- Temp directory cleanup via trap
- `set -euo pipefail` to catch all errors
- Downloads over HTTPS only

### Version pinning example

```bash
FREESPACE_VERSION=v0.1.0 curl -fsSL .../install.sh | bash
```

## Verification

- [ ] `curl -fsSL .../install.sh | bash` installs successfully on a clean machine
- [ ] `freespace --version` outputs correct version
- [ ] Checksum verification works (tamper with archive to confirm failure)
- [ ] `FREESPACE_VERSION=v0.1.0` pins to correct version
- [ ] `FREESPACE_INSTALL_DIR=/tmp/test` installs to custom location
- [ ] PATH warning appears when `~/.local/bin` is not in PATH
- [ ] Script fails cleanly on Linux with clear error message
- [ ] Test on both Apple Silicon and Intel Macs

## Depends on

- `release-workflow.md` -- needs published GitHub Releases with archives + checksums
