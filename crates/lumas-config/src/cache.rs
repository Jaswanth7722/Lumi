//! # Validated Config Cache
//!
//! Thread-safe, lock-free config cache using `ArcSwap`.
//! Readers never block. Writers atomically replace the `Arc`.

use crate::resolver::ResolvedConfig;
use crate::schema::LumiConfig;
use arc_swap::ArcSwap;
use std::sync::Arc;

/// Thread-safe, lock-free config cache.
///
/// Readers call `current()` to get an `Arc<LumiConfig>` — O(1), no locking.
/// Writers call `store()` to atomically replace the config.
pub struct ConfigCache {
    /// The current validated config.
    inner: ArcSwap<LumiConfig>,
    /// The resolved config with source annotations.
    resolved: ArcSwap<ResolvedConfig>,
}

impl Clone for ConfigCache {
    fn clone(&self) -> Self {
        Self {
            inner: ArcSwap::new(self.inner.load_full()),
            resolved: ArcSwap::new(self.resolved.load_full()),
        }
    }
}

impl ConfigCache {
    /// Create a new config cache with the given initial config.
    pub fn new(config: LumiConfig) -> Self {
        Self {
            inner: ArcSwap::new(Arc::new(config)),
            resolved: ArcSwap::new(Arc::new(ResolvedConfig::default())),
        }
    }

    /// Returns a snapshot of the current config.
    /// O(1) — just an Arc clone. No locking.
    pub fn current(&self) -> Arc<LumiConfig> {
        self.inner.load_full()
    }

    /// Returns the resolved config with source annotations.
    pub fn resolved(&self) -> Arc<ResolvedConfig> {
        self.resolved.load_full()
    }

    /// Replace the entire config atomically.
    /// Called only by ConfigLoader and ConfigWatcher.
    pub(crate) fn store(&self, config: LumiConfig, resolved: ResolvedConfig) {
        self.inner.store(Arc::new(config));
        self.resolved.store(Arc::new(resolved));
    }

    /// Replace only the LumiConfig (preserving existing resolved config).
    #[allow(dead_code)]
    pub(crate) fn store_config(&self, config: LumiConfig) {
        self.inner.store(Arc::new(config));
    }
}
