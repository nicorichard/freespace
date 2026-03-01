// Application configuration.

/// Application-level configuration.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub dry_run: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { dry_run: true }
    }
}
