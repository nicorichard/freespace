mod app;
mod config;
mod core;
mod module;
mod tui;

use clap::{Parser, Subcommand};

/// Interactive terminal interface for browsing and cleaning disk space consumers.
#[derive(Parser)]
#[command(name = "freespace", version, about)]
struct Cli {
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
    /// Install a module from a path or URL
    Install,
    /// List installed modules
    List,
    /// Remove an installed module
    Remove,
    /// Inspect a module's manifest and targets
    Inspect,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // Install panic hook to restore terminal on panic
            tui::install_panic_hook();

            // Initialize terminal
            let mut terminal = tui::init()?;

            // Create app and run the main event loop
            let mut app = app::App::new();
            app.run(&mut terminal)?;

            // Restore terminal on normal exit
            tui::restore()?;
        }
        Some(Command::Scan) => {
            println!("scan: not yet implemented");
        }
        Some(Command::Module { command }) => match command {
            ModuleCommand::Install => println!("module install: not yet implemented"),
            ModuleCommand::List => println!("module list: not yet implemented"),
            ModuleCommand::Remove => println!("module remove: not yet implemented"),
            ModuleCommand::Inspect => println!("module inspect: not yet implemented"),
        },
    }

    Ok(())
}
