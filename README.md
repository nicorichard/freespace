# Freespace

Interactive TUI for browsing and cleaning disk space consumers.

## Install

```sh
cargo build --release
```

The binary will be at `target/release/freespace`.

## Usage

Launch the TUI:

```sh
freespace
```

### Subcommands

```sh
freespace module list   # List installed modules
freespace module install <path>
freespace module remove <name>
freespace module inspect <name>
```

## Keybindings

| Key | Action |
|---|---|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Enter` | Open module detail |
| `Space` | Toggle item selection (detail view) |
| `a` | Select all items |
| `n` | Deselect all items |
| `c` | Cleanup selected items |
| `d` | Dry-run cleanup |
| `Backspace` / `Esc` | Go back |
| `?` | Help overlay |
| `q` | Quit |

## Built-in Modules

- **Xcode Derived Data** — `~/Library/Developer/Xcode/DerivedData` (macOS)
- **npm/yarn/pnpm Cache** — npm cache, `~/.yarn/cache`, pnpm store
- **Homebrew Cache** — `~/Library/Caches/Homebrew` (macOS)
- **Docker** — Docker Desktop disk images and build cache
- **General Caches** — `~/Library/Caches`, `~/.cache`

## License

MIT
