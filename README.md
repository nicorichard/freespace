# Freespace

Your disk fills up with caches, build artifacts, and leftovers from dozens of dev tools. Each has its own cleanup dance. Freespace is a single TUI that finds all of it — Docker images, Xcode derived data, node_modules, simulator runtimes, package caches — shows you what it's worth, and reclaims the space you pick.

![freespace demo](docs/demo.gif)

## Install

### With [Homebrew](https://brew.sh)

```sh
brew install nicorichard/tap/freespace
```

### With [mise](https://mise.jdx.dev)

```sh
mise use -g github:nicorichard/freespace@latest
```

### From source

```sh
cargo build --release
# Binary at target/release/freespace
```

## Why Freespace

**Complete out of the box** — Ships knowing about 50+ tools: Docker, Xcode, npm, Homebrew, Gradle, Steam, and the rest. No install step.

**Declarative & safe** — Every module is a plain TOML manifest. A manifest declares paths, or names one of freespace's built-in handlers; it can never specify a command of its own. You can read and audit every module in seconds.

**Correct, not just thorough** — Some things break if you just delete their directory. Simulator devices and runtimes are removed through `xcrun simctl`, which keeps CoreSimulator's registry consistent, and freespace shows you the exact command before it runs anything.

**Extensible** — Add a TOML module for anything freespace doesn't already cover.

## What it cleans

Run `freespace` and it scans everything it knows about for your platform. That covers language toolchains (Rust, Go, Node, Python, Ruby, PHP, Haskell, Elixir, Swift, Kotlin, .NET), build systems (Gradle, Maven, Bazel, SBT, ccache), containers, IDEs and editors, iOS and Android development, browsers, games, chat and creative apps, and the various caches macOS and Linux leave behind.

To see the full list, and what a given entry actually looks at:

```sh
freespace module list
freespace module inspect docker
```

To take one out of the rotation:

```sh
freespace module disable steam
freespace module enable steam
```

## Heads up

Freespace deletes files. We require confirmation before any cleanup, but please review what you're removing and keep backups of anything important. Use at your own risk — see the MIT license for details.

## Configuration

Freespace can be configured via `~/.config/freespace/config.toml`. See the [configuration docs](docs/configuration.md) for all available options.

## Extending freespace

For paths freespace doesn't cover, write a module: a directory with a `module.toml` declaring what to scan. Drop it in `~/.config/freespace/modules/`, or install one from any Git repo:

```sh
freespace module install github:owner/repo
```

A module whose `id` matches a built-in replaces it, which is how you customise a shipped module. See [MODULES.md](MODULES.md) for the manifest format.

## License

MIT
