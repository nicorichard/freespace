pub mod theme;
pub mod views;
pub mod widgets;

use std::io::{self, stdout, Stdout};
use std::panic;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

/// Type alias for the terminal backend used throughout the application.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize the terminal for TUI rendering.
///
/// Enables raw mode, enters the alternate screen, enables mouse capture,
/// and returns a configured Terminal instance.
pub fn init() -> io::Result<Tui> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::new(backend)
}

/// Restore the terminal to its original state.
///
/// Disables raw mode, leaves the alternate screen, disables mouse capture,
/// and shows the cursor.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        crossterm::cursor::Show
    )?;
    Ok(())
}

/// Install a panic hook that restores the terminal before printing the panic message.
///
/// This prevents the terminal from being left in a broken state after a panic.
pub fn install_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();
        original_hook(panic_info);
    }));
}
