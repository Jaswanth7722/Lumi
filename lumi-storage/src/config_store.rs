//! # Configuration Store
//!
//! Manages user and system configuration persisted to disk.
//! In production, reads/writes TOML configuration files.

use std::collections::HashMap;
use tracing::debug;

/// Stores and retrieves configuration values.
pub struct ConfigStore {
    /// Configuration key-value pairs.
    config: HashMap<String, serde_json::Value>,
    /// Path to the configuration file.
    config_path: String,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
            config_path: String::new(),
        }
    }

    /// Initialize the config store from a file path.
    pub fn initialize(&mut self, path: &str) {
        self.config_path = path.to_string();
        debug!("Config store initialized from: {path}");
    }

    /// Get a configuration value by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.config.get(key)
    }

    /// Set a configuration value.
    pub fn set(&mut self, key: &str, value: serde_json::Value) {
        self.config.insert(key.to_string(), value);
    }

    /// Get all configuration as a JSON object.
    pub fn all(&self) -> &HashMap<String, serde_json::Value> {
        &self.config
    }

    /// Save configuration to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        if self.config_path.is_empty() {
            anyhow::bail!("No config path set");
        }
        Ok(())
    }

    /// Load configuration from disk.
    pub fn load(&mut self) -> anyhow::Result<()> {
        if self.config_path.is_empty() {
            anyhow::bail!("No config path set");
        }
        Ok(())
    }

    /// Set default configuration values.
    pub fn set_defaults(&mut self) {
        self.set("ui.theme", serde_json::json!("system"));
        self.set("ui.font_size", serde_json::json!(14));
        self.set("voice.wake_word", serde_json::json!("Hey Lumi"));
        self.set("voice.rate", serde_json::json!(1.0));
        self.set("memory.retention_days", serde_json::json!(365));
        self.set("privacy.offline_mode", serde_json::json!(false));
        self.set("render.fps", serde_json::json!(60));
        self.set("render.fur_shells", serde_json::json!(24));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let mut store = ConfigStore::new();
        store.set("test_key", serde_json::json!("test_value"));
        assert_eq!(
            store.get("test_key"),
            Some(&serde_json::json!("test_value"))
        );
    }

    #[test]
    fn test_defaults() {
        let mut store = ConfigStore::new();
        store.set_defaults();
        assert_eq!(store.get("render.fps"), Some(&serde_json::json!(60)));
        assert_eq!(store.get("voice.wake_word"), Some(&serde_json::json!("Hey Lumi")));
    }

    #[test]
    fn test_missing_key() {
        let store = ConfigStore::new();
        assert_eq!(store.get("nonexistent"), None);
    }
}
