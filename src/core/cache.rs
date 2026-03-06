// Persistent size cache for faster subsequent scans.
//
// Stores `path -> (size, timestamp)` entries in a JSON file at
// `~/.config/freespace/cache.json`. On startup, cached sizes are used
// immediately while a background scan refreshes them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config;

const CACHE_FILENAME: &str = "cache.json";

/// A single cached size entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    /// Unix timestamp (seconds) when the size was recorded.
    pub cached_at: u64,
}

/// In-memory size cache backed by a JSON file.
#[derive(Debug, Clone)]
pub struct SizeCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

impl SizeCache {
    /// Create an empty cache (for tests or first run).
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load the cache from disk, or return an empty cache if it doesn't exist / is corrupt.
    pub fn load() -> Self {
        let entries = cache_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { entries }
    }

    /// Look up a cached size for the given path.
    pub fn get(&self, path: &Path) -> Option<u64> {
        self.entries.get(path).map(|e| e.size)
    }

    /// Insert or update a cache entry.
    pub fn set(&mut self, path: PathBuf, size: u64) {
        let cached_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.entries.insert(path, CacheEntry { size, cached_at });
    }

    /// Remove an entry (e.g. after deletion).
    pub fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Persist the cache to disk. Errors are silently ignored.
    pub fn save(&self) {
        if let Some(path) = cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string(&self.entries) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

fn cache_path() -> Option<PathBuf> {
    config::config_dir().map(|d| d.join(CACHE_FILENAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn set_and_get() {
        let mut cache = SizeCache {
            entries: HashMap::new(),
        };
        let path = PathBuf::from("/tmp/test");
        cache.set(path.clone(), 42);
        assert_eq!(cache.get(&path), Some(42));
    }

    #[test]
    fn get_missing() {
        let cache = SizeCache {
            entries: HashMap::new(),
        };
        assert_eq!(cache.get(Path::new("/nonexistent")), None);
    }

    #[test]
    fn remove_entry() {
        let mut cache = SizeCache {
            entries: HashMap::new(),
        };
        let path = PathBuf::from("/tmp/test");
        cache.set(path.clone(), 100);
        cache.remove(&path);
        assert_eq!(cache.get(&path), None);
    }

    #[test]
    fn roundtrip_save_load() {
        let tmp = TempDir::new().unwrap();
        let cache_file = tmp.path().join("cache.json");

        let mut entries = HashMap::new();
        entries.insert(
            PathBuf::from("/some/path"),
            CacheEntry {
                size: 1234,
                cached_at: 1000,
            },
        );

        let json = serde_json::to_string(&entries).unwrap();
        std::fs::write(&cache_file, &json).unwrap();

        let loaded: HashMap<PathBuf, CacheEntry> =
            serde_json::from_str(&std::fs::read_to_string(&cache_file).unwrap()).unwrap();
        assert_eq!(loaded.get(Path::new("/some/path")).unwrap().size, 1234);
    }
}
