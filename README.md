# Freespace

Your disk fills up with caches, build artifacts, and leftovers from dozens of dev tools. Each has its own cleanup dance. Freespace gives you a single TUI to reclaim that space — powered by community-written cleanup modules.

![freespace demo](docs/demo.gif)

## Why Freespace

**Community cleanup protocols** — Anyone can write and share a module. If you use a tool that accumulates disk waste, chances are someone's already written a module for it.

**Declarative & safe** — Modules are plain TOML manifests. No scripts, no code execution. You can read and audit every module in seconds.

**Git-native sharing** — Install modules from any GitHub repo with one command. Fork, customize, share back.

## What a module looks like

A module is a `module.toml` that describes what to scan and clean. Here's a real one:

```toml
name = "Xcode Derived Data"
version = "1.0.0"
description = "Xcode build artifacts and derived data. Safe to delete; Xcode regenerates on next build."
author = "freespace"
platforms = ["macos"]

[[targets]]
path = "~/Library/Developer/Xcode/DerivedData/*"
description = "Xcode derived data directories for each project"
```

That's it. No scripts, no plugins — just a declaration of where disk space hides.

## Install

### With [mise](https://mise.jdx.dev)

```sh
mise use -g github:nicorichard/freespace@latest
```

### From source

```sh
cargo build --release
# Binary at target/release/freespace
```

## Built-in Modules

These ship with freespace and work out of the box:

- **Xcode Derived Data** — Xcode build artifacts (macOS)
- **Node Package Caches** — npm, Yarn, and pnpm caches
- **Homebrew Cache** — `~/Library/Caches/Homebrew` (macOS)
- **Docker** — Docker Desktop disk images and build cache
- **General Caches** — `~/Library/Caches`, `~/.cache`

## Community Modules

Install a module from GitHub:

```sh
freespace module install github:owner/repo
```

### Create your own

1. Create a directory with a `module.toml`
2. Define your targets — paths or directory patterns to scan
3. Push to GitHub
4. Anyone can install it with `freespace module install github:you/your-module`

See the built-in modules for examples of the manifest format.

## License

MIT
