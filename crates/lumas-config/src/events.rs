//! # Config Event Definitions
//!
//! Standalone event types for configuration lifecycle events.
//! These do not depend on lumas-runtime's Event trait to avoid circular dependencies.
//! The lumas-runtime crate bridges these to its event bus via adapter types.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

/// Emitted when configuration is successfully loaded during bootstrap.
#[derive(Debug, Clone)]
pub struct ConfigLoaded {
    /// Path to the config file (None if no file was found).
    pub path: Option<PathBuf>,
    /// Schema version that was loaded.
    pub schema_version: u32,
    /// Count of fields from each source.
    pub source_summary: HashMap<String, u32>,
    /// When the load occurred.
    pub loaded_at: DateTime<Utc>,
}

impl ConfigLoaded {
    /// Create a new ConfigLoaded event.
    pub fn new(path: Option<PathBuf>, schema_version: u32) -> Self {
        Self {
            path,
            schema_version,
            source_summary: HashMap::new(),
            loaded_at: Utc::now(),
        }
    }
}

/// Emitted when hot reload successfully applies a new config version.
#[derive(Debug, Clone)]
pub struct ConfigReloaded {
    /// Dotted paths of changed fields.
    pub changed_keys: Vec<String>,
    /// When the reload occurred.
    pub reloaded_at: DateTime<Utc>,
}

impl ConfigReloaded {
    /// Create a new ConfigReloaded event.
    pub fn new(changed_keys: Vec<String>) -> Self {
        Self {
            changed_keys,
            reloaded_at: Utc::now(),
        }
    }
}

/// Emitted when hot reload fails validation or parsing.
#[derive(Debug, Clone)]
pub struct ConfigReloadFailed {
    /// Error description.
    pub error: String,
    /// Whether the previous config version was retained.
    pub previous_version_retained: bool,
    /// When the failure occurred.
    pub failed_at: DateTime<Utc>,
}

impl ConfigReloadFailed {
    /// Create a new ConfigReloadFailed event.
    pub fn new(error: String) -> Self {
        Self {
            error,
            previous_version_retained: true,
            failed_at: Utc::now(),
        }
    }
}

/// Emitted when a migration is applied during load.
#[derive(Debug, Clone)]
pub struct ConfigMigrated {
    /// Schema version migrated from.
    pub from_version: u32,
    /// Schema version migrated to.
    pub to_version: u32,
    /// Descriptions of migrations applied.
    pub migrations_applied: Vec<String>,
    /// When the migration occurred.
    pub migrated_at: DateTime<Utc>,
}

impl ConfigMigrated {
    /// Create a new ConfigMigrated event.
    pub fn new(from_version: u32, to_version: u32, migrations_applied: Vec<String>) -> Self {
        Self {
            from_version,
            to_version,
            migrations_applied,
            migrated_at: Utc::now(),
        }
    }
}

/// Emitted when validation finds warnings or auto-correctable errors.
#[derive(Debug, Clone)]
pub struct ConfigValidationWarning {
    /// Warning messages.
    pub warnings: Vec<String>,
    /// Descriptions of fields that were auto-corrected.
    pub auto_corrected: Vec<String>,
}

/// A generic trait for publishing config events.
/// Implemented by lumas-runtime to bridge to its typed event bus.
#[async_trait::async_trait]
pub trait ConfigEventPublisher: Send + Sync {
    /// Called when config is loaded during bootstrap.
    async fn on_config_loaded(&self, event: ConfigLoaded);
    /// Called when config is hot-reloaded successfully.
    async fn on_config_reloaded(&self, event: ConfigReloaded);
    /// Called when config hot-reload fails.
    async fn on_config_reload_failed(&self, event: ConfigReloadFailed);
    /// Called when config migration runs.
    async fn on_config_migrated(&self, event: ConfigMigrated);
}
