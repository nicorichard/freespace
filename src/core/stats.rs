// Lightweight cleanup stats tracking.
//
// Persists total and per-module freed bytes in `~/.config/freespace/stats.json`.
// Silently ignores all errors — stats must never block cleanup.

use crate::config;

/// In-memory cleanup statistics.
pub struct Stats {
    pub total_freed: u64,
    /// Per-module freed bytes: (module_id, bytes).
    modules: Vec<(String, u64)>,
}

impl Stats {
    /// Load stats from disk. Returns empty stats on any error.
    pub fn load() -> Self {
        try_load().unwrap_or_else(|| Stats {
            total_freed: 0,
            modules: Vec::new(),
        })
    }

    /// Persist stats to disk. Silently ignores errors.
    fn save(&self) {
        let _ = try_save(self);
    }

    /// Record freed bytes for a module, updating both the module entry and the
    /// global total, then persist to disk.
    pub fn record(&mut self, module_id: &str, bytes: u64) {
        if bytes == 0 {
            return;
        }
        self.total_freed += bytes;
        if let Some(entry) = self.modules.iter_mut().find(|(id, _)| id == module_id) {
            entry.1 += bytes;
        } else {
            self.modules.push((module_id.to_string(), bytes));
        }
        self.save();
    }

    /// Look up the all-time freed bytes for a specific module.
    pub fn module_total(&self, module_id: &str) -> u64 {
        self.modules
            .iter()
            .find(|(id, _)| id == module_id)
            .map(|(_, b)| *b)
            .unwrap_or(0)
    }
}

fn stats_path() -> Option<std::path::PathBuf> {
    config::config_dir().map(|d| d.join("stats.json"))
}

fn try_load() -> Option<Stats> {
    let path = stats_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    parse_stats(&content)
}

fn try_save(stats: &Stats) -> Option<()> {
    let path = stats_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = format_stats(stats);
    std::fs::write(&path, json).ok()
}

/// Format stats as a JSON string (manual, no serde_json dependency).
fn format_stats(stats: &Stats) -> String {
    let mut out = format!("{{\"total_freed\":{}", stats.total_freed);
    out.push_str(",\"modules\":{");
    for (i, (id, bytes)) in stats.modules.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":{}", escape_json(id), bytes));
    }
    out.push_str("}}\n");
    out
}

/// Parse stats from a JSON string.
fn parse_stats(json: &str) -> Option<Stats> {
    let total_freed = extract_number(json, "total_freed")?;

    let modules_start = json.find("\"modules\"")?;
    let rest = &json[modules_start..];
    let brace_start = rest.find('{')?;
    let inner = &rest[brace_start + 1..];

    let mut modules = Vec::new();
    let mut pos = 0;
    let bytes = inner.as_bytes();
    while pos < bytes.len() {
        // Find next key
        let key_start = match inner[pos..].find('"') {
            Some(i) => pos + i + 1,
            None => break,
        };
        let key_end = match inner[key_start..].find('"') {
            Some(i) => key_start + i,
            None => break,
        };
        let key = &inner[key_start..key_end];

        // Find colon then number
        let after_key = key_end + 1;
        let colon = match inner[after_key..].find(':') {
            Some(i) => after_key + i + 1,
            None => break,
        };

        // Parse number
        let num_start = inner[colon..].find(|c: char| c.is_ascii_digit());
        let num_start = match num_start {
            Some(i) => colon + i,
            None => break,
        };
        let num_end = inner[num_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| num_start + i)
            .unwrap_or(inner.len());
        if let Ok(val) = inner[num_start..num_end].parse::<u64>() {
            modules.push((key.to_string(), val));
        }

        pos = num_end;
        // Stop at closing brace
        if inner[pos..].starts_with('}') {
            break;
        }
    }

    Some(Stats {
        total_freed,
        modules,
    })
}

/// Extract a top-level numeric value by key from JSON.
fn extract_number(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\":", key);
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let num_start = rest.find(|c: char| c.is_ascii_digit())?;
    let num_end = rest[num_start..]
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[num_start..num_start + num_end].parse().ok()
}

/// Escape special characters for JSON string values.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty() {
        let stats = Stats {
            total_freed: 0,
            modules: Vec::new(),
        };
        let json = format_stats(&stats);
        let parsed = parse_stats(&json).unwrap();
        assert_eq!(parsed.total_freed, 0);
        assert!(parsed.modules.is_empty());
    }

    #[test]
    fn round_trip_with_data() {
        let stats = Stats {
            total_freed: 5_000_000,
            modules: vec![
                ("docker".to_string(), 3_000_000),
                ("xcode".to_string(), 2_000_000),
            ],
        };
        let json = format_stats(&stats);
        let parsed = parse_stats(&json).unwrap();
        assert_eq!(parsed.total_freed, 5_000_000);
        assert_eq!(parsed.modules.len(), 2);
        assert_eq!(parsed.modules[0], ("docker".to_string(), 3_000_000));
        assert_eq!(parsed.modules[1], ("xcode".to_string(), 2_000_000));
    }

    #[test]
    fn record_accumulates() {
        let mut stats = Stats {
            total_freed: 0,
            modules: Vec::new(),
        };
        stats.total_freed += 100;
        stats.modules.push(("docker".to_string(), 100));

        // Simulate a second record
        stats.total_freed += 200;
        if let Some(entry) = stats.modules.iter_mut().find(|(id, _)| id == "docker") {
            entry.1 += 200;
        }

        assert_eq!(stats.total_freed, 300);
        assert_eq!(stats.module_total("docker"), 300);
        assert_eq!(stats.module_total("unknown"), 0);
    }

    #[test]
    fn module_total_lookup() {
        let stats = Stats {
            total_freed: 500,
            modules: vec![("a".to_string(), 200), ("b".to_string(), 300)],
        };
        assert_eq!(stats.module_total("a"), 200);
        assert_eq!(stats.module_total("b"), 300);
        assert_eq!(stats.module_total("c"), 0);
    }

    #[test]
    fn parse_handles_garbage() {
        assert!(parse_stats("not json").is_none());
        assert!(parse_stats("").is_none());
    }

    #[test]
    fn escape_json_special_chars() {
        assert_eq!(escape_json("hello"), "hello");
        assert_eq!(escape_json("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn record_zero_is_noop() {
        let mut stats = Stats {
            total_freed: 100,
            modules: vec![("a".to_string(), 100)],
        };
        // record with 0 bytes should not change anything
        // (record calls save which would fail in tests without a config dir,
        //  but zero-byte records return early before save)
        stats.record("a", 0);
        assert_eq!(stats.total_freed, 100);
        assert_eq!(stats.module_total("a"), 100);
    }
}
