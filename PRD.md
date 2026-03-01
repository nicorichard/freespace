# Freespace — Product Requirements Document

## Product Overview

Freespace is a TUI/CLI application that helps developers reclaim hard drive space through a community-driven module system. It scans for caches, build artifacts, simulators, and other space consumers from developer tools, presents them in an interactive terminal UI, and enables safe cleanup with dry-run previews.

**Value proposition:** One tool to find and clean up disk usage from *all* your developer tools — no more googling cache paths or writing one-off scripts.

## User Personas

### 1. Maya — iOS Developer

- Works on 3-4 Xcode projects, uses CocoaPods and SPM
- Has old simulator runtimes and DerivedData from projects she no longer maintains
- Regularly runs low on disk space on her 256GB MacBook
- Wants: quick way to reclaim Xcode-related space without breaking active projects

### 2. Jake — Full-Stack Developer

- Works across React (npm/yarn), Python (venv/pip), and Go
- Runs Docker for local development
- Has accumulated caches across multiple package managers
- Wants: a single view of all cache usage across his toolchains

### 3. Priya — DevOps Engineer

- Maintains CI/CD infrastructure, works with Docker, Terraform, Homebrew
- Manages multiple machines
- Wants: a scriptable tool she can also run non-interactively for automated cleanup

## User Stories — MVP

### Discovery & Visibility

- **US-1:** As a developer, I want to see a list of all detected space consumers on my machine so I can understand where my disk space is going.
- **US-2:** As a developer, I want to see the total size for each category (e.g., "Xcode DerivedData — 12.3 GB") so I can prioritize what to clean.
- **US-3:** As a developer, I want to drill into a category and see its component parts (e.g., individual project DerivedData folders) so I can make informed decisions.

### Cleanup

- **US-4:** As a developer, I want to select specific items for cleanup so I can keep what I need and remove what I don't.
- **US-5:** As a developer, I want to preview what will be deleted before confirming (dry-run) so I don't accidentally remove important data.
- **US-6:** As a developer, I want to see how much space was reclaimed after cleanup so I know the action was effective.

### Module Management

- **US-7:** As a developer, I want to install community modules from GitHub so I can extend Freespace to cover tools it doesn't support out of the box.
- **US-8:** As a developer, I want to list and remove installed modules so I can manage my Freespace configuration.
- **US-9:** As a developer, I want to create my own modules so I can add support for tools specific to my workflow.

## MVP Feature Set

### Core TUI

- **Module list view** — main screen showing all active modules with aggregate sizes, sorted by size descending
- **Detail view** — drill-down into a module showing individual discovered items with sizes
- **Selection** — keyboard-driven item selection (toggle individual items, select all, deselect all)
- **Cleanup action** — execute deletion of selected items with confirmation dialog
- **Dry-run mode** — enabled by default; shows what *would* be deleted without actually deleting
- **Progress indicators** — async size calculation with real-time streaming updates in the UI
- **Keyboard navigation** — vim-style keybindings (j/k, enter, q, space for select)
- **Help overlay** — keybinding reference accessible via `?`

### Module System

- **Module loading** — parse `module.toml` manifests with declarative target definitions
- **Static discovery** — glob-based path matching defined in TOML
- **Module validation** — verify manifest structure on install

### Module Management CLI

- `freespace module install github:user/repo@version` — install or upgrade a module from GitHub (reinstalling an existing module at a new version upgrades it in place)
- `freespace module list` — show installed modules with versions
- `freespace module remove <name>` — uninstall a module
- `freespace module inspect <name>` — show full manifest contents

### Built-in Starter Modules

The following modules ship with the binary and serve as both useful defaults and reference implementations:

1. **Xcode DerivedData** — `~/Library/Developer/Xcode/DerivedData/`
2. **npm/yarn/pnpm cache** — npm cache dir, `~/.yarn/cache`, pnpm store
3. **Homebrew cache** — `~/Library/Caches/Homebrew/`
4. **Docker** — Docker Desktop disk images and build cache
5. **General caches** — `~/Library/Caches/` and `~/.cache/` aggregation

### CLI Interface

- `freespace` — launch the TUI (default)
- `freespace scan` — run discovery and print results to stdout
- `freespace module <subcommand>` — module management
- `freespace --help` — usage information
- `freespace --version` — version display

## Future Features

These are explicitly **out of scope for MVP** but inform architectural decisions:

- **Dynamic module scripting** — sandboxed Lua scripts for complex discovery cases (e.g., reading Docker `daemon.json` for custom paths). Enables modules to go beyond static glob patterns when needed
- **Module registry** — centralized, searchable index of community modules
- **Sanity Validation** — rulesets to pre-empt and deny or warn for dangerous operations (e.g. deleting the home directory)
- **Local/project scanning** — register project directories and crawl them for project-relative artifacts (node_modules, target/, .build/, venv/). Support `freespace <path>` for ad-hoc scans and a project registry (`freespace project add ~/Projects`) for persistent roots
- **Scheduled cleanup** — cron-like automated cleanup with configurable rules
- **Disk usage trends** — track space usage over time, surface growth patterns
- **Shell completions** — bash, zsh, fish completions for all commands
- **Non-interactive mode** — `freespace clean --all --dry-run` for scripting and CI
- **Trash integration** — move to trash instead of permanent deletion on supported platforms
- **Module categories/tags** — organize modules by language, tool type, platform
- **Configuration profiles** — save and share cleanup configurations across machines

## Non-Functional Requirements

### Performance

- **Startup time:** TUI visible within 500ms of launch
- **Scan responsiveness:** size calculations run asynchronously; UI updates as results stream in
- **Large directories:** handle directories with millions of files without freezing the UI
- **Memory:** bounded memory usage even when scanning very large directory trees

### Distribution

- **Single binary:** no runtime dependencies, no installation of interpreters or frameworks
- **Cross-compilation:** target macOS (aarch64, x86_64) for MVP; Linux as a fast follow

### Security & Safety

- **No root required:** all standard operations work without sudo/root
- **Permission errors:** gracefully skip inaccessible directories with warnings, never crash
- **Dry-run default:** destructive operations require explicit opt-in
- **User confirmation:** all deletions require interactive confirmation (unless `--yes` flag in future non-interactive mode)
- **Module sandboxing:** modules are declarative TOML manifests with no code execution

### Usability

- **Terminal compatibility:** works in common terminal emulators (Terminal.app, iTerm2, Alacritty, Kitty, WezTerm)
- **Responsive layout:** adapts to terminal width (minimum 80 columns)
- **Color support:** 256-color and truecolor with graceful fallback
- **Error messages:** clear, actionable error messages for common failure modes
