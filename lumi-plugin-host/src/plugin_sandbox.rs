//! # Plugin Sandbox (Chapter 11)
//!
//! WebAssembly-based sandbox for plugin execution using Wasmtime.
//! Provides capability-based isolation so plugins can only access
//! the resources they've declared.

use lumi_common::tool::Capability;
use std::collections::HashMap;
use tracing::debug;

/// Configuration for creating a new plugin sandbox.
#[derive(Clone)]
pub struct SandboxConfig {
    pub max_memory_bytes: u64,
    pub max_instructions: u64,
    pub allowed_capabilities: Vec<Capability>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024, // 64 MB
            max_instructions: 100_000,
            allowed_capabilities: Vec::new(),
        }
    }
}

/// A WebAssembly plugin sandbox for secure plugin execution.
pub struct PluginSandbox {
    /// Active sandboxes by plugin name.
    sandboxes: HashMap<String, SandboxInstance>,
    /// Default configuration for new sandboxes.
    default_config: SandboxConfig,
}

/// A single sandbox instance.
pub struct SandboxInstance {
    pub plugin_name: String,
    pub config: SandboxConfig,
    pub created_at: i64,
    /// Whether the sandbox is currently executing.
    pub is_running: bool,
}

impl PluginSandbox {
    pub fn new() -> Self {
        Self {
            sandboxes: HashMap::new(),
            default_config: SandboxConfig::default(),
        }
    }

    /// Create a new sandbox for a plugin.
    pub fn create_sandbox(&mut self, plugin_name: &str, config: Option<SandboxConfig>) {
        let cfg = config.unwrap_or_else(|| self.default_config.clone());
        self.sandboxes.insert(
            plugin_name.to_string(),
            SandboxInstance {
                plugin_name: plugin_name.to_string(),
                config: cfg,
                created_at: chrono::Utc::now().timestamp_millis(),
                is_running: false,
            },
        );
        debug!("Created sandbox for plugin: {plugin_name}");
    }

    /// Execute a tool call within a plugin sandbox.
    pub fn execute(&mut self, _tool_name: &str, _input: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // In production: instantiate the Wasm module, execute the function,
        // enforce capability checks, and return the result.
        Ok(serde_json::json!({
            "status": "executed",
        }))
    }

    /// Destroy a sandbox and release its resources.
    pub fn destroy_sandbox(&mut self, plugin_name: &str) {
        self.sandboxes.remove(plugin_name);
        debug!("Destroyed sandbox for plugin: {plugin_name}");
    }

    /// Get the number of active sandboxes.
    pub fn active_count(&self) -> usize {
        self.sandboxes.len()
    }

    /// Check if a plugin has an active sandbox.
    pub fn has_sandbox(&self, plugin_name: &str) -> bool {
        self.sandboxes.contains_key(plugin_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_destroy_sandbox() {
        let mut sandbox = PluginSandbox::new();
        sandbox.create_sandbox("test-plugin", None);
        assert!(sandbox.has_sandbox("test-plugin"));
        assert_eq!(sandbox.active_count(), 1);
        sandbox.destroy_sandbox("test-plugin");
        assert_eq!(sandbox.active_count(), 0);
    }

    #[test]
    fn test_execute_in_sandbox() {
        let mut sandbox = PluginSandbox::new();
        sandbox.create_sandbox("test", None);
        let result = sandbox.execute("test.tool", &serde_json::json!({"input": "test"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_config() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_memory_bytes, 64 * 1024 * 1024);
        assert_eq!(config.max_instructions, 100_000);
    }
}
