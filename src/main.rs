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
                    println!("Installing from {}...", source);
                    match module::installer::install(&source, &modules_dir) {
                        Ok(results) => {
                            for r in &results {
                                let action = if r.was_upgrade {
                                    "Updated"
                                } else {
                                    "Installed"
                                };
                                println!(
                                    "  {} {} v{} -> {}",
                                    action,
                                    r.name,
                                    r.version,
                                    r.installed_to.display()
                                );
                            }
                            println!("\n{} module(s) installed successfully.", results.len());
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    }
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
