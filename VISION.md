# Freespace — Project Vision

## Problem

Developers accumulate enormous disk usage from the tools they rely on every day. Xcode DerivedData, CocoaPods caches, npm/yarn/pnpm caches, Docker images, Homebrew downloads, Rust target directories, Python virtual environments, Android SDK components, simulator runtimes — the list goes on. Over time, these consume tens or even hundreds of gigabytes.

Cleaning up is painful:

- **Scattered** — every tool stores caches in a different location with different conventions
- **Manual** — developers resort to googling "how to clear Xcode cache" repeatedly
- **Error-prone** — deleting the wrong directory can break builds or lose configuration
- **Invisible** — most of this usage is hidden in `~/Library`, `~/.cache`, or dotfile directories that never show up in Finder

There is no single tool that understands the full landscape of developer disk usage across toolchains, languages, and platforms.

## Solution

**Freespace** is a terminal UI application that aggregates disk usage across all developer tools, caches, and build artifacts into a single interactive interface. It scans, categorizes, and presents cleanup opportunities — powered by a community-driven module system.

Instead of hardcoding knowledge about every tool, Freespace uses **modules** — small, declarative descriptions of where tools store data and how to safely clean them up. Anyone can write and share a module. The core engine handles all scanning, display, and deletion; modules just describe *what* to look for and *where*.

The experience: launch `freespace`, see a ranked list of space consumers, drill into any category, preview what would be cleaned, and reclaim space with a single keypress.

## Core Principles

### 1. Modularity

Every cleanup target is a module. There are no hardcoded tools or paths in the core binary. Built-in modules ship alongside the binary but follow the exact same format as community modules. This keeps the core small and the coverage extensible.

### 2. Security

Modules cannot execute arbitrary code. The core engine handles all destructive filesystem operations. Modules describe *what* to find using declarative TOML manifests and, when dynamic discovery is needed, sandboxed Lua scripts with a read-only API. Users can audit any module by reading its manifest — it's a small, human-readable file.

### 3. Community-Driven

Anyone can create and publish a module via GitHub. Installing a module is a single command. The ecosystem grows organically: if a developer uses a tool that Freespace doesn't cover yet, they can write a module in minutes and share it.

### 4. Speed

Filesystem scanning is fast and non-blocking. Size calculations happen asynchronously with streaming updates in the UI. Modules are loaded lazily. The TUI remains responsive even when scanning large directory trees.

### 5. Safety

Dry-run is the default mode. Before any deletion, users see exactly what will be removed and how much space will be reclaimed. Confirmation is explicit. Where possible, operations are reversible (e.g., moving to trash instead of permanent deletion). Permission errors are handled gracefully — Freespace never asks for sudo.

## Target Audience

**Primary:** developers on macOS who work across multiple toolchains — iOS/macOS (Xcode, simulators, CocoaPods, SPM), web (Node.js, npm/yarn/pnpm), backend (Rust, Go, Python, Java), and infrastructure (Docker, Homebrew).

**Secondary:** any developer on macOS or Linux who wants visibility into hidden disk usage from development tools.

The sweet spot is the developer who works on multiple projects across multiple tech stacks, where disk usage balloons silently and cleanup is a periodic, frustrating chore.

## Long-Term Vision

Freespace becomes the **package manager for disk reclamation** — a community ecosystem where:

- Modules are published, versioned, and discoverable through a central registry
- Popular modules are audited and promoted
- Disk usage trends are tracked over time, surfacing growth patterns
- Cleanup can be automated on a schedule
- Teams can share module sets to standardize development environment hygiene
- The tool works seamlessly across macOS and Linux

The core stays small and fast. The value compounds with every module the community contributes.
