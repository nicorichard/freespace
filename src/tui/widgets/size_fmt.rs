// Human-readable file size formatting utilities.

const KB: u64 = 1_000;
const MB: u64 = 1_000_000;
const GB: u64 = 1_000_000_000;
const TB: u64 = 1_000_000_000_000;

/// Format a byte count into a human-readable string.
///
/// - B, KB, MB: no decimal places (e.g. "847 MB")
/// - GB, TB: one decimal place (e.g. "12.3 GB")
pub fn format_size(bytes: u64) -> String {
    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Format an optional size, returning a placeholder for unknown or error states.
///
/// - `Some(size)` → formatted size string
/// - `None` → `"..."` (still calculating / unknown)
///
/// For error states, callers should use `"N/A"` directly.
pub fn format_size_or_placeholder(size: Option<u64>) -> String {
    match size {
        Some(bytes) => format_size(bytes),
        None => "...".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn test_kilobytes() {
        assert_eq!(format_size(1_000), "1 KB");
        assert_eq!(format_size(1_500), "1 KB");
        assert_eq!(format_size(999_999), "999 KB");
    }

    #[test]
    fn test_megabytes() {
        assert_eq!(format_size(1_000_000), "1 MB");
        assert_eq!(format_size(847_000_000), "847 MB");
        assert_eq!(format_size(999_999_999), "999 MB");
    }

    #[test]
    fn test_gigabytes() {
        assert_eq!(format_size(1_000_000_000), "1.0 GB");
        assert_eq!(format_size(12_300_000_000), "12.3 GB");
        assert_eq!(format_size(999_900_000_000), "999.9 GB");
    }

    #[test]
    fn test_terabytes() {
        assert_eq!(format_size(1_000_000_000_000), "1.0 TB");
        assert_eq!(format_size(2_500_000_000_000), "2.5 TB");
    }

    #[test]
    fn test_placeholder_some() {
        assert_eq!(format_size_or_placeholder(Some(1_000_000)), "1 MB");
        assert_eq!(format_size_or_placeholder(Some(12_300_000_000)), "12.3 GB");
    }

    #[test]
    fn test_placeholder_none() {
        assert_eq!(format_size_or_placeholder(None), "...");
    }
}
