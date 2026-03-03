// Audit trail for cleanup operations.

use std::path::Path;

use crate::config;
use crate::tui::widgets::format_size;

/// Append a cleanup operation to the audit log.
///
/// Format: `2026-03-03T14:22:01Z TRASH ~/Library/Caches/docker (2.1 GB) [module: docker]`
///
/// Silently ignores errors — audit logging must never block cleanup.
pub fn log_operation(op: &str, path: &Path, size: Option<u64>, module_id: &str) {
    let _ = try_log(op, path, size, module_id);
}

/// Maximum audit log size in bytes (1 MB). When exceeded, the oldest entries
/// are discarded to bring the file back to roughly half the limit.
const MAX_LOG_BYTES: u64 = 1_024 * 1_024;

fn try_log(op: &str, path: &Path, size: Option<u64>, module_id: &str) -> Option<()> {
    use std::io::Write;

    let log_path = config::config_dir()?.join("audit.log");

    // Ensure parent directory exists
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Rotate if the log exceeds the size limit
    rotate_if_needed(&log_path);

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok()?;

    // Collapse home prefix for readability
    let display_path = collapse_home(path);

    let size_str = match size {
        Some(s) => format!(" ({})", format_size(s)),
        None => String::new(),
    };

    let now = now_iso8601();

    writeln!(
        file,
        "{} {} {}{} [module: {}]",
        now, op, display_path, size_str, module_id
    )
    .ok()?;

    Some(())
}

/// Truncate the log file when it exceeds `MAX_LOG_BYTES`, keeping the most
/// recent half of lines. Silently does nothing on any error.
fn rotate_if_needed(log_path: &Path) {
    let meta = match std::fs::metadata(log_path) {
        Ok(m) => m,
        Err(_) => return,
    };

    if meta.len() <= MAX_LOG_BYTES {
        return;
    }

    let content = match std::fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let lines: Vec<&str> = content.lines().collect();
    let keep_from = lines.len() / 2;
    let truncated = lines[keep_from..].join("\n");

    let _ = std::fs::write(log_path, format!("{}\n", truncated));
}

/// Replace the home directory prefix with `~` for display.
fn collapse_home(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(suffix) = path.strip_prefix(&home) {
            return format!("~/{}", suffix.display());
        }
    }
    path.display().to_string()
}

/// Return current UTC time in ISO 8601 format without pulling in chrono.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert epoch seconds to date-time components
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Shift to March-based year for easier leap-year handling
    let mut year = 1970;
    loop {
        let year_days = if is_leap(year) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }

    let month_days: &[u64] = if is_leap(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i as u64 + 1;
            break;
        }
        days -= md;
    }

    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_iso8601_format() {
        let ts = now_iso8601();
        // Should match pattern YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn collapse_home_with_home_prefix() {
        if let Some(home) = dirs::home_dir() {
            let path = home.join("Library").join("Caches");
            let collapsed = collapse_home(&path);
            assert!(collapsed.starts_with("~/"));
            assert!(collapsed.contains("Library"));
        }
    }

    #[test]
    fn collapse_home_without_home_prefix() {
        let collapsed = collapse_home(Path::new("/tmp/something"));
        assert_eq!(collapsed, "/tmp/something");
    }

    #[test]
    fn log_creates_file_and_writes() {
        // This test uses the real config dir, but just verifies the format function
        // doesn't panic. The actual file write depends on config_dir() being writable.
        // We test the formatting logic instead.
        let ts = now_iso8601();
        assert!(!ts.is_empty());
    }

    #[test]
    fn rotate_truncates_large_log() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("audit.log");

        // Write more than MAX_LOG_BYTES
        let line = "2026-01-01T00:00:00Z TRASH ~/cache (1 KB) [module: test]\n";
        let count = (MAX_LOG_BYTES as usize / line.len()) + 100;
        let content: String = line.repeat(count);
        fs::write(&log, &content).unwrap();
        assert!(fs::metadata(&log).unwrap().len() > MAX_LOG_BYTES);

        rotate_if_needed(&log);

        let after = fs::read_to_string(&log).unwrap();
        let lines_before = content.lines().count();
        let lines_after = after.lines().count();
        assert!(
            lines_after < lines_before,
            "should have fewer lines after rotation"
        );
        // Should keep roughly half
        assert!(lines_after >= lines_before / 2 - 1);
        assert!(lines_after <= lines_before / 2 + 1);
    }

    #[test]
    fn rotate_noop_when_small() {
        use std::fs;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let log = tmp.path().join("audit.log");

        let content = "2026-01-01T00:00:00Z TRASH ~/cache (1 KB) [module: test]\n";
        fs::write(&log, content).unwrap();

        rotate_if_needed(&log);

        let after = fs::read_to_string(&log).unwrap();
        assert_eq!(after, content);
    }

    #[test]
    fn days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-03-03 is day 20515 since epoch
        // 1970..2025 = 55 years, let's compute: verified via external tool
        let (y, m, d) = days_to_ymd(20515);
        assert_eq!((y, m, d), (2026, 3, 3));
    }
}
