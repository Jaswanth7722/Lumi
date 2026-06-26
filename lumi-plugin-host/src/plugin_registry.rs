//! # Plugin Registry (Chapter 11)
//!
//! Manages plugin registration, capability declarations,
//! version tracking, and plugin lifecycle.

use lumi_common::tool::{Capability, ToolDefinition};
use std::collections::HashMap;
use tracing::debug;

/// Metadata about a registered plugin.
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: String,
    pub declared_capabilities: Vec<Capability>,
    pub tools: Vec<ToolDefinition>,
}

/// Registry of all installed and available plugins.
pub struct PluginRegistry {
    /// Registered plugins by name.
    plugins: HashMap<String, PluginMetadata>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a new plugin.
    pub fn register(&mut self, metadata: PluginMetadata) -> bool {
        if self.plugins.contains_key(&metadata.name) {
            debug!("Plugin already registered: {}", metadata.name);
            return false;
        }
        self.plugins.insert(metadata.name.clone(), metadata);
        true
    }

    /// Unregister a plugin by name.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.plugins.remove(name).is_some()
    }

    /// Get a plugin's metadata.
    pub fn get(&self, name: &str) -> Option<&PluginMetadata> {
        self.plugins.get(name)
    }

    /// List all registered plugin names.
    pub fn list(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Get all tool definitions from registered plugins.
    pub fn all_tools(&self) -> Vec<&ToolDefinition> {
        self.plugins.values().flat_map(|p| p.tools.iter()).collect()
    }

    /// Check if a plugin is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Get the number of registered plugins.
    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_list() {
        let mut registry = PluginRegistry::new();
        assert!(registry.register(PluginMetadata {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            author: Some("Lumi".into()),
            description: "A test plugin".into(),
            declared_capabilities: vec![],
            tools: vec![],
        }));
        assert!(registry.contains("test-plugin"));
        assert_eq!(registry.count(), 1);
        assert!(registry.list().contains(&"test-plugin"));
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = PluginRegistry::new();
        let meta = PluginMetadata {
            name: "dup".into(),
            version: "1.0.0".into(),
            author: None,
            description: "".into(),
            declared_capabilities: vec![],
            tools: vec![],
        };
        assert!(registry.register(meta));
        assert!(!registry.register(PluginMetadata {
            name: "dup".into(),
            version: "2.0.0".into(),
            author: None,
            description: "".into(),
            declared_capabilities: vec![],
            tools: vec![],
        }));
    }

    #[test]
    fn test_unregister() {
        let mut registry = PluginRegistry::new();
        registry.register(PluginMetadata {
            name: "temp".into(),
            version: "1.0.0".into(),
            author: None,
            description: "".into(),
            declared_capabilities: vec![],
            tools: vec![],
        });
        assert!(registry.unregister("temp"));
        assert_eq!(registry.count(), 0);
    }
}
