mod app;
mod config;
mod core;
mod module;
mod tui;

use std::fs;

use clap::{Parser, Subcommand};

/// Interactive terminal interface for browsing and cleaning disk space consumers.
#[derive(Parser)]
#[command(name = "freespace", version, about)]
struct Cli {
    /// Additional module directory to scan (can be repeated)
    #[arg(long = "module-dir", global = true)]
    module_dirs: Vec<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Scan for disk space consumers and report results
    Scan,
    /// Manage freespace modules
    Module {
        #[command(subcommand)]
        command: ModuleCommand,
    },
}

#[derive(Subcommand)]
enum ModuleCommand {
    /// Install a module from a GitHub source
    Install {
        /// Source identifier (e.g. github:user/repo@v1.0.0#module-name)
        source: String,
    },
    /// List installed modules
    List,
    /// Remove an installed module
    Remove {
        /// Name of the module to remove
        name: String,
    },
    /// Inspect a module's manifest and source
    Inspect {
        /// Name of the module to inspect
        name: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // Install panic hook to restore terminal on panic
            tui::install_panic_hook();

            // Initialize terminal
            let mut terminal = tui::init()?;

            // Create app and run the main event loop
            let mut app = app::App::new(cli.module_dirs);
            app.run(&mut terminal)?;

            // Restore terminal on normal exit
            tui::restore()?;
        }
        Some(Command::Scan) => {
            println!("scan: not yet implemented");
        }
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
                                let action = if r.was_upgrade { "Updated" } else { "Installed" };
                                println!(
                                    "  {} {} v{} -> {}",
                                    action,
                                    r.name,
                                    r.version,
                                    r.installed_to.display()
                                );
                            }
                            println!(
                                "\n{} module(s) installed successfully.",
                                results.len()
                            );
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
                ModuleCommand::Remove { name } => {
                    cmd_remove(&modules_dir, &name)?;
                }
                ModuleCommand::Inspect { name } => {
                    cmd_inspect(&modules_dir, &name)?;
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
        let module: module::manifest::Module = match toml::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let source = module::installer::read_source_info(&path);

        if !found {
            println!("{:<20} {:<10} {}", "NAME", "VERSION", "SOURCE");
            found = true;
        }

        let source_str = match source {
            Some(s) => s.repository,
            None => "local".to_string(),
        };

        println!("{:<20} {:<10} {}", module.name, module.version, source_str);
    }

    if !found {
        println!("No modules installed.");
    }
}

/// Remove an installed module by name.
fn cmd_remove(modules_dir: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let module_dir = find_module_dir(modules_dir, name)?;
    fs::remove_dir_all(&module_dir)?;
    println!("Removed module '{}'.", name);
    Ok(())
}

/// Inspect an installed module's manifest and source information.
fn cmd_inspect(modules_dir: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let module_dir = find_module_dir(modules_dir, name)?;

    let manifest_content = fs::read_to_string(module_dir.join("module.toml"))?;
    let module: module::manifest::Module = toml::from_str(&manifest_content)?;

    println!("Module: {}", module.name);
    println!("Version: {}", module.version);
    println!("Description: {}", module.description);
    println!("Author: {}", module.author);
    println!("Platforms: {}", module.platforms.join(", "));
    println!();

    println!("Targets:");
    for target in &module.targets {
        let desc = target
            .description
            .as_deref()
            .unwrap_or("(no description)");
        println!("  {} - {}", target.path, desc);
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

/// Find a module directory by module name (checks both directory name and manifest name).
fn find_module_dir(
    modules_dir: &std::path::Path,
    name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    // First try direct directory name match
    let direct = modules_dir.join(name);
    if direct.is_dir() && direct.join("module.toml").exists() {
        return Ok(direct);
    }

    // Fall back to scanning manifests for matching name
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
                if let Ok(module) = toml::from_str::<module::manifest::Module>(&content) {
                    if module.name == name {
                        return Ok(path);
                    }
                }
            }
        }
    }

    anyhow::bail!("module '{}' not found", name)
}
