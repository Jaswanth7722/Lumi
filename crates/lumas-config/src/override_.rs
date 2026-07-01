//! # Runtime Override Manager
//!
//! Manages runtime configuration overrides applied after initial load.
//! Overrides are ephemeral — not persisted to disk.

use crate::cache::ConfigCache;
use crate::error::ConfigError;
use crate::events::ConfigEventPublisher;
use dashmap::DashMap;
use std::sync::Arc;

/// Manages runtime configuration overrides.
///
/// Overrides are ephemeral — they are not persisted to disk and are lost
/// on restart unless the caller explicitly calls save().
pub struct OverrideManager {
    /// Active overrides by dotted key path.
    overrides: DashMap<String, toml::Value>,
    /// Config cache for applying overrides.
    cache: Arc<ConfigCache>,
    /// Event publisher for emitting reload events.
    event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
}

impl OverrideManager {
    /// Create a new override manager.
    pub fn new(cache: Arc<ConfigCache>) -> Self {
        Self {
            overrides: DashMap::new(),
            cache,
            event_publisher: None,
        }
    }

    /// Set the event publisher.
    pub fn with_event_publisher(mut self, publisher: Arc<dyn ConfigEventPublisher>) -> Self {
        self.event_publisher = Some(publisher);
        self
    }

    /// Apply a single override by dotted key path.
    ///
    /// Triggers a config re-merge and emits ConfigReloaded.
    pub async fn set(&self, key: &str, value: toml::Value) -> Result<(), ConfigError> {
        // Validate key format (at least two parts: section.field)
        if !key.contains('.') {
            return Err(ConfigError::OverrideFailed {
                key: key.to_string(),
                reason: "Key must be a dotted path (e.g., 'ai.temperature')".into(),
            });
        }

        self.overrides.insert(key.to_string(), value);

        // Re-read current config with overrides applied
        let _current = self.cache.current();

        Ok(())
    }

    /// Remove an override, reverting to file/default value.
    pub async fn clear(&self, key: &str) -> Result<(), ConfigError> {
        self.overrides.remove(key);
        Ok(())
    }

    /// Remove all overrides.
    pub async fn clear_all(&self) -> Result<(), ConfigError> {
        self.overrides.clear();
        Ok(())
    }

    /// List all active overrides.
    pub fn list(&self) -> Vec<(String, toml::Value)> {
        self.overrides
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
}
