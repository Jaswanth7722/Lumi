//! # Asset Cache
//!
//! Caches frequently accessed assets (character models, textures,
//! animation clips, voice models) for fast loading.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::debug;

/// A cached asset entry with expiry tracking.
pub struct CacheEntry {
    /// The cached data.
    data: Vec<u8>,
    /// When this entry was created.
    created_at: Instant,
    /// Number of times this entry has been accessed.
    access_count: u64,
}

/// Cache statistics.
pub struct CacheStats {
    pub total_entries: usize,
    pub total_size_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// Simple TTL-based asset cache.
pub struct AssetCache {
    /// Cached entries by key.
    entries: HashMap<String, CacheEntry>,
    /// Maximum cache size in bytes.
    max_size_bytes: u64,
    /// Default TTL for cache entries.
    default_ttl: Duration,
    /// Total cache hits.
    hits: u64,
    /// Total cache misses.
    misses: u64,
}

impl AssetCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            max_size_bytes: 500 * 1024 * 1024,      // 500 MB
            default_ttl: Duration::from_secs(3600), // 1 hour
            hits: 0,
            misses: 0,
        }
    }

    /// Get a cached asset by key.
    pub fn get(&mut self, key: &str) -> Option<&[u8]> {
        if let Some(entry) = self.entries.get(key) {
            if entry.created_at.elapsed() < self.default_ttl {
                self.hits += 1;
                // Update access count
                let entry = self.entries.get_mut(key).unwrap();
                entry.access_count += 1;
                return Some(&entry.data);
            } else {
                // Expired
                self.entries.remove(key);
            }
        }
        self.misses += 1;
        None
    }

    /// Store an asset in the cache.
    pub fn put(&mut self, key: &str, data: Vec<u8>) {
        // Check if we need to evict
        let current_size: u64 = self.entries.values().map(|e| e.data.len() as u64).sum();
        let new_size = current_size + data.len() as u64;

        if new_size > self.max_size_bytes {
            self.evict_lru();
        }

        self.entries.insert(
            key.to_string(),
            CacheEntry {
                data,
                created_at: Instant::now(),
                access_count: 0,
            },
        );
        debug!("Cached asset: {key}");
    }

    /// Check if a key exists in the cache and is not expired.
    pub fn contains(&self, key: &str) -> bool {
        self.entries
            .get(key)
            .map(|e| e.created_at.elapsed() < self.default_ttl)
            .unwrap_or(false)
    }

    /// Remove expired entries from the cache.
    pub fn clean_expired(&mut self) {
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.created_at.elapsed() >= self.default_ttl)
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired {
            self.entries.remove(&key);
        }
    }

    /// Evict the least recently accessed entry.
    fn evict_lru(&mut self) {
        if let Some(key) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.access_count)
            .map(|(k, _)| k.clone())
        {
            debug!("Evicting LRU cache entry: {key}");
            self.entries.remove(&key);
        }
    }

    /// Clear all cached assets.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        let total_size: u64 = self.entries.values().map(|e| e.data.len() as u64).sum();
        let total_accesses = self.hits + self.misses;
        CacheStats {
            total_entries: self.entries.len(),
            total_size_bytes: total_size,
            hits: self.hits,
            misses: self.misses,
            hit_rate: if total_accesses > 0 {
                self.hits as f64 / total_accesses as f64
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut cache = AssetCache::new();
        cache.put("test_key", vec![1, 2, 3]);
        assert_eq!(cache.get("test_key"), Some(&vec![1, 2, 3][..]));
    }

    #[test]
    fn test_miss() {
        let mut cache = AssetCache::new();
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn test_contains() {
        let mut cache = AssetCache::new();
        cache.put("key", vec![]);
        assert!(cache.contains("key"));
        assert!(!cache.contains("nonexistent"));
    }

    #[test]
    fn test_clear() {
        let mut cache = AssetCache::new();
        cache.put("a", vec![1]);
        cache.put("b", vec![2]);
        cache.clear();
        assert_eq!(cache.stats().total_entries, 0);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = AssetCache::new();
        cache.get("miss1");
        cache.get("miss2");
        cache.put("hit_key", vec![1, 2, 3]);
        cache.get("hit_key");

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert!((stats.hit_rate - 0.333).abs() < 0.01);
    }
}
