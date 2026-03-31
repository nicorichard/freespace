use std::fs;

use clap::{Parser, Subcommand};

use freespace::app;
use freespace::config;
use freespace::module;
use freespace::tui;

/// Interactive terminal interface for browsing and cleaning disk space consumers.
#[derive(Parser)]
#[command(name = "freespace", version, about)]
struct Cli {
    /// Directory to scan (overrides configured search_dirs)
    #[arg(value_name = "PATH")]
    path: Option<String>,

    /// Additional module directory to scan (can be repeated)
    #[arg(long = "module-dir", global = true)]
    module_dirs: Vec<String>,

    /// Directory to search for local targets (can be repeated)
    #[arg(long = "search-dir", global = true)]
    search_dirs: Vec<String>,

    /// Simulate cleanup without actually deleting anything
    #[arg(long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Manage freespace modules
    Module {
        #[command(subcommand)]
        command: ModuleCommand,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Add a directory to search for local targets
    AddSearchDir {
        /// Path to add
        path: String,
    },
    /// Remove a directory from search dirs
    RemoveSearchDir {
        /// Path to remove
        path: String,
    },
    /// Show current configuration
    List,
}

#[derive(Subcommand)]
enum ModuleCommand {
    /// Install a module from a source
    Install {
        /// Source (e.g. github:user/repo@v1.0.0#module-name or /path/to/module)
        source: String,
    },
    /// List installed modules
    List,
    /// Remove an installed module
    Remove {
        /// ID of the module to remove
        id: String,
    },
    /// Inspect a module's manifest and source
    Inspect {
        /// ID of the module to inspect
        id: String,
    },
    /// Update installed modules from their sources
    Update {
        /// ID of a specific module to update (updates all if omitted)
        id: Option<String>,
    },
    /// Check for available updates without applying them
    Outdated,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // If a positional path was provided, add it to search_dirs
            let directory_mode = cli.path.is_some();
            let mut search_dirs = cli.search_dirs;
            if let Some(path) = cli.path {
                search_dirs.push(path);
            }

            // Install panic hook to restore terminal on panic
            tui::install_panic_hook();

            // Initialize terminal
            let mut terminal = tui::init()?;

            // Create app and run the main event loop
            let mut app = app::App::new(cli.module_dirs, search_dirs, cli.dry_run, directory_mode);
            app.run(&mut terminal)?;

            // Restore terminal on normal exit
            tui::restore()?;

            // Report any paths that were blocked by safety rules
            let blocked = app.blocked_paths();
            if !blocked.is_empty() {
                eprintln!();
                eprintln!(
                    "note: {} path{} blocked by safety rules:",
                    blocked.len(),
                    if blocked.len() == 1 { " was" } else { "s were" }
                );
                for (path, reason, id, name) in blocked {
                    eprintln!("  {} ({}, from {} [{}])", path.display(), reason, name, id);
                }
            }
        }
        Some(Command::Config { command }) => match command {
            ConfigCommand::AddSearchDir { path } => {
                let mut cfg = config::AppConfig::load()?;
                if cfg.add_search_dir(path.clone()) {
                    cfg.save()?;
                    println!("Added '{}' to search_dirs.", path);
                } else {
                    println!("'{}' is already in search_dirs.", path);
                }
            }
            ConfigCommand::RemoveSearchDir { path } => {
                let mut cfg = config::AppConfig::load()?;
                if cfg.remove_search_dir(&path) {
                    cfg.save()?;
                    println!("Removed '{}' from search_dirs.", path);
                } else {
                    println!("'{}' is not in search_dirs.", path);
                }
            }
            ConfigCommand::List => {
                let cfg = config::AppConfig::load()?;
                println!("search_dirs:");
                if cfg.search_dirs.is_empty() {
                    println!("  (none)");
                } else {
                    for d in &cfg.search_dirs {
                        println!("  {}", d);
                    }
                }
                println!("module_dirs:");
                if cfg.module_dirs.is_empty() {
                    println!("  (none)");
                } else {
                    for d in &cfg.module_dirs {
                        println!("  {}", d);
                    }
                }
                println!("audit_log: {}", cfg.audit_log);
                println!("enforce_scope: {}", cfg.enforce_scope);
                if !cfg.protected_paths.is_empty() {
                    println!("protected_paths:");
                    for p in &cfg.protected_paths {
                        println!("  {}", p);
                    }
                }
            }
        },
        Some(Command::Module { command }) => {
            let modules_dir = config::default_modules_dir()
                .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
            fs::create_dir_all(&modules_dir)?;

            match command {
                ModuleCommand::Install { source } => {
                    tui::install_panic_hook();
                    let mut terminal = tui::init()?;
                    let mut app = app::App::new_for_install(source, modules_dir.clone());
                    app.run(&mut terminal)?;
                    tui::restore()?;
                }
                ModuleCommand::List => {
                    cmd_list(&modules_dir);
                }
                ModuleCommand::Remove { id } => {
                    cmd_remove(&modules_dir, &id)?;
                }
                ModuleCommand::Inspect { id } => {
                    cmd_inspect(&modules_dir, &id)?;
                }
                ModuleCommand::Update { id } => {
                    cmd_update(&modules_dir, id.as_deref())?;
                }
                ModuleCommand::Outdated => {
                    cmd_outdated(&modules_dir);
                }
            }
        }
    }

    Ok(())
}

/// List all installed modules with source information.
fn cmd_list(modules_dir: &std::path::Path) {
    let entries = match fs::read_dir(modules_dir) {
        Ok(e) => e,
        Err(_) => {
            println!("No modules installed.");
            return;
        }
    };

    let mut found = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("module.toml");
        if !manifest_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let module = match module::manifest::Module::parse(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let source = module::installer::read_source_info(&path);

        if !found {
            println!("{:<20} {:<20} {:<10} SOURCE", "ID", "NAME", "VERSION");
            found = true;
        }

        let source_str = match source {
            Some(s) => s.repository,
            None => "local".to_string(),
        };

        println!(
            "{:<20} {:<20} {:<10} {}",
            module.id, module.name, module.version, source_str
        );
    }

    if !found {
        println!("No modules installed.");
    }
}

/// Remove an installed module by id.
fn cmd_remove(modules_dir: &std::path::Path, id: &str) -> anyhow::Result<()> {
    let module_dir = find_module_dir(modules_dir, id)?;
    fs::remove_dir_all(&module_dir)?;
    println!("Removed module '{}'.", id);
    Ok(())
}

/// Inspect an installed module's manifest and source information.
fn cmd_inspect(modules_dir: &std::path::Path, id: &str) -> anyhow::Result<()> {
    let module_dir = find_module_dir(modules_dir, id)?;

    let manifest_content = fs::read_to_string(module_dir.join("module.toml"))?;
    let module = module::manifest::Module::parse(&manifest_content)?;

    println!("Id: {}", module.id);
    println!("Module: {}", module.name);
    println!("Version: {}", module.version);
    println!("Description: {}", module.description);
    println!("Author: {}", module.author);
    println!("Platforms: {}", module.platforms.join(", "));
    println!();

    println!("Targets:");
    for target in &module.targets {
        let desc = target.description.as_deref().unwrap_or("(no description)");
        println!("  {} - {}", target.paths.join(", "), desc);
    }

    if let Some(source) = module::installer::read_source_info(&module_dir) {
        println!();
        println!("Source:");
        println!("  Repository: {}", source.repository);
        if let Some(ref git_ref) = source.git_ref {
            println!("  Ref: {}", git_ref);
        }
        println!("  Commit: {}", source.commit);
        if let Some(ref path) = source.path {
            println!("  Path: {}", path);
        }
        println!("  Installed at: {}", source.installed_at);
    }

    Ok(())
}

/// Update installed modules from their sources.
fn cmd_update(modules_dir: &std::path::Path, id: Option<&str>) -> anyhow::Result<()> {
    let dirs = collect_module_dirs(modules_dir, id)?;

    let mut updated = 0u32;
    let mut up_to_date = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;

    for dir in &dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        print!("Checking {}... ", name);

        let status = module::installer::update_module(dir, modules_dir);
        match status {
            module::installer::UpdateStatus::Updated {
                name,
                old_version,
                new_version,
                old_commit,
                new_commit,
                ..
            } => {
                let version_change = if !old_version.is_empty() && !new_version.is_empty() {
                    format!("v{} -> v{}, ", old_version, new_version)
                } else {
                    String::new()
                };
                println!(
                    "updated {} ({}{})",
                    name,
                    version_change,
                    format_commit_range(&old_commit, &new_commit)
                );
                updated += 1;
            }
            module::installer::UpdateStatus::NewerTag {
                name,
                current_tag,
                latest_tag,
                ..
            } => {
                println!(
                    "{} has newer tag available ({} -> {}). Reinstall with: freespace module install {}@{}",
                    name, current_tag, latest_tag, name, latest_tag
                );
                skipped += 1;
            }
            module::installer::UpdateStatus::UpToDate { .. } => {
                println!("up to date");
                up_to_date += 1;
            }
            module::installer::UpdateStatus::Skipped { reason, .. } => {
                println!("skipped ({})", reason);
                skipped += 1;
            }
            module::installer::UpdateStatus::Failed { reason, .. } => {
                println!("error: {}", reason);
                failed += 1;
            }
        }
    }

    println!();
    let mut parts = Vec::new();
    if updated > 0 {
        parts.push(format!("{} updated", updated));
    }
    if up_to_date > 0 {
        parts.push(format!("{} up to date", up_to_date));
    }
    if skipped > 0 {
        parts.push(format!("{} skipped", skipped));
    }
    if failed > 0 {
        parts.push(format!("{} failed", failed));
    }
    println!("{} module(s): {}", dirs.len(), parts.join(", "));

    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Check for available updates without applying them.
fn cmd_outdated(modules_dir: &std::path::Path) {
    let dirs = match collect_module_dirs(modules_dir, None) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let mut has_updates = false;

    for dir in &dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        print!("Checking {}... ", name);

        let status = module::installer::check_update(dir);
        match status {
            module::installer::UpdateStatus::Updated {
                name,
                old_commit,
                new_commit,
                ..
            } => {
                println!(
                    "update available for {} ({})",
                    name,
                    format_commit_range(&old_commit, &new_commit)
                );
                has_updates = true;
            }
            module::installer::UpdateStatus::NewerTag {
                name,
                current_tag,
                latest_tag,
                ..
            } => {
                println!("{} has newer tag ({} -> {})", name, current_tag, latest_tag);
                has_updates = true;
            }
            module::installer::UpdateStatus::UpToDate { .. } => {
                println!("up to date");
            }
            module::installer::UpdateStatus::Skipped { reason, .. } => {
                println!("skipped ({})", reason);
            }
            module::installer::UpdateStatus::Failed { reason, .. } => {
                println!("error: {}", reason);
            }
        }
    }

    if has_updates {
        println!("\nRun `freespace module update` to apply updates.");
    } else {
        println!("\nAll modules are up to date.");
    }
}

/// Collect module directories, optionally filtered by id.
fn collect_module_dirs(
    modules_dir: &std::path::Path,
    id: Option<&str>,
) -> anyhow::Result<Vec<std::path::PathBuf>> {
    if let Some(id) = id {
        let dir = find_module_dir(modules_dir, id)?;
        return Ok(vec![dir]);
    }

    let entries = fs::read_dir(modules_dir)?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("module.toml").exists() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Format a commit range as "abc123..def456" using short SHAs.
fn format_commit_range(old: &str, new: &str) -> String {
    let short_old = if old.len() > 7 { &old[..7] } else { old };
    let short_new = if new.len() > 7 { &new[..7] } else { new };
    format!("{}..{}", short_old, short_new)
}

/// Find a module directory by module id (checks both directory name and manifest id).
fn find_module_dir(modules_dir: &std::path::Path, id: &str) -> anyhow::Result<std::path::PathBuf> {
    // First try direct directory name match
    let direct = modules_dir.join(id);
    if direct.is_dir() && direct.join("module.toml").exists() {
        return Ok(direct);
    }

    // Fall back to scanning manifests for matching id
    if let Ok(entries) = fs::read_dir(modules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("module.toml");
            if !manifest_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(module) = module::manifest::Module::parse(&content) {
                    if module.id == id {
                        return Ok(path);
                    }
                }
            }
        }
    }

    anyhow::bail!("module '{}' not found", id)
}
