//! # ConfigManager — Public Entry Point
//!
//! Owns the config cache, watcher, override manager, and migration engine.
//! Constructed once during bootstrap and stored in RuntimeContext.

use crate::cache::ConfigCache;
use crate::error::ConfigError;
use crate::events::ConfigEventPublisher;
use crate::loader::ConfigLoader;
use crate::migration::MigrationEngine;
use crate::override_::OverrideManager;
use crate::platform;
use crate::schema::LumiConfig;
use std::path::PathBuf;
use std::sync::Arc;

/// The public entry point for the entire configuration system.
///
/// ConfigManager owns the config cache, watcher, override manager, and
/// migration engine. It is constructed once during bootstrap and stored
/// in RuntimeContext.
///
/// # Thread Safety
///
/// ConfigManager is Clone (cheap Arc clone) and Send + Sync.
/// All methods are safe to call from any thread or async task.
#[derive(Clone)]
pub struct ConfigManager {
    /// Inner implementation (Arc-based for cheap cloning).
    inner: Arc<ConfigManagerInner>,
}

struct ConfigManagerInner {
    /// Config cache for lock-free reads.
    cache: ConfigCache,
    /// Override manager for runtime overrides.
    overrides: OverrideManager,
    /// Migration engine for schema upgrades.
    _migrations: MigrationEngine,
    /// Event publisher for config lifecycle events.
    event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
    /// The path the config was loaded from.
    config_path: Option<PathBuf>,
}

impl ConfigManager {
    /// Load configuration from the default platform path.
    pub async fn load(
        event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
    ) -> Result<Self, ConfigError> {
        let loader = ConfigLoader::new();
        Self::load_inner(loader, event_publisher).await
    }

    /// Load from an explicit path.
    pub async fn load_from(
        path: PathBuf,
        event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
    ) -> Result<Self, ConfigError> {
        let loader = ConfigLoader::new().with_path(path);
        Self::load_inner(loader, event_publisher).await
    }

    /// Internal load implementation.
    async fn load_inner(
        loader: ConfigLoader,
        event_publisher: Option<Arc<dyn ConfigEventPublisher>>,
    ) -> Result<Self, ConfigError> {
        let loader = if let Some(ref publisher) = event_publisher {
            loader.with_event_publisher(publisher.clone())
        } else {
            loader
        };

        let (cache, _config) = loader.load().await?;

        let overrides = OverrideManager::new(Arc::new(cache.clone()));
        let overrides = if let Some(ref publisher) = event_publisher {
            overrides.with_event_publisher(publisher.clone())
        } else {
            overrides
        };

        Ok(Self {
            inner: Arc::new(ConfigManagerInner {
                cache,
                overrides,
                _migrations: MigrationEngine::new(),
                event_publisher,
                config_path: None,
            }),
        })
    }

    /// Returns the current validated configuration snapshot. O(1).
    pub fn current(&self) -> Arc<LumiConfig> {
        self.inner.cache.current()
    }

    /// Start the file watcher for hot reload. Must be called after load().
    pub async fn start_watching(&self) -> Result<(), ConfigError> {
        let config_path = self.inner.config_path.clone().unwrap_or_else(|| {
            platform::config_file_path().unwrap_or_else(|_| PathBuf::from("config.toml"))
        });

        let manager = self.clone();
        tokio::spawn(async move {
            let watcher = crate::watcher::ConfigWatcher::new(config_path, Arc::new(manager));
            if let Err(e) = watcher.run().await {
                tracing::error!("Config watcher failed: {e}");
            }
        });

        Ok(())
    }

    /// Apply a runtime override.
    pub async fn set_override(&self, key: &str, value: toml::Value) -> Result<(), ConfigError> {
        self.inner.overrides.set(key, value).await
    }

    /// Save the current config to disk, creating a .bak backup first.
    pub async fn save(&self) -> Result<(), ConfigError> {
        let path = self.inner.config_path.clone().unwrap_or_else(|| {
            platform::config_file_path().unwrap_or_else(|_| PathBuf::from("config.toml"))
        });

        let config = self.current();
        let toml_str = config.to_toml().map_err(|e| ConfigError::WriteFailed {
            path: path.clone(),
            source: std::io::Error::other(e.to_string()),
        })?;

        // Create backup if file exists
        if path.exists() {
            let bak_path = path.with_extension("toml.bak");
            std::fs::copy(&path, &bak_path).map_err(|e| ConfigError::BackupFailed {
                path: bak_path,
                source: e,
            })?;
        }

        std::fs::write(&path, &toml_str)
            .map_err(|e| ConfigError::WriteFailed { path, source: e })?;

        Ok(())
    }

    /// Export config to JSON string (sensitive fields redacted automatically).
    pub fn export_json(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self.current().as_ref()).map_err(|e| {
            ConfigError::WriteFailed {
                path: PathBuf::from("<json>"),
                source: std::io::Error::other(e.to_string()),
            }
        })
    }

    /// Check whether a runtime feature flag is enabled.
    pub fn is_feature_enabled(&self, flag: &str) -> bool {
        self.current().feature_flags.is_enabled(flag)
    }

    /// Get the event publisher reference (used by ConfigWatcher).
    pub fn event_publisher(&self) -> Option<Arc<dyn ConfigEventPublisher>> {
        self.inner.event_publisher.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_loads_defaults() {
        let manager = ConfigManager::load(None).await.unwrap();
        let config = manager.current();
        assert_eq!(config.general.language, "en");
    }

    #[tokio::test]
    async fn test_export_json_succeeds() {
        let manager = ConfigManager::load(None).await.unwrap();
        let json = manager.export_json().unwrap();
        assert!(json.contains("general"));
        assert!(!json.contains("anthropic_api_key")); // Secret fields skip_serializing
    }

    #[test]
    fn test_is_feature_enabled() {
        let config = LumiConfig::default();
        assert!(config.feature_flags.is_enabled("voice"));
        assert!(!config.feature_flags.is_enabled("nonexistent"));
    }
}
