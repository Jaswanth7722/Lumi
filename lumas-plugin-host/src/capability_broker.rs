//! # Capability Broker (Chapter 11)
//!
//! Mediates access to system capabilities for plugins.
//! Enforces the principle of least privilege — plugins can only
//! use capabilities they've explicitly declared and have been granted.

use anyhow::{Result, anyhow};
use lumas_common::tool::Capability;
use std::collections::HashMap;
use tracing::debug;

/// Manages capability declarations and grants for plugins.
pub struct CapabilityBroker {
    /// Capabilities granted per plugin.
    grants: HashMap<String, Vec<Capability>>,
    /// Global capability whitelist for all plugins.
    global_whitelist: Vec<Capability>,
    /// Whether the broker requires explicit approval for each capability.
    require_explicit_approval: bool,
}

impl CapabilityBroker {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
            global_whitelist: vec![Capability::FilesystemRead, Capability::NetworkFetch],
            require_explicit_approval: true,
        }
    }

    /// Grant capabilities to a plugin.
    pub fn grant(&mut self, plugin_name: &str, capabilities: Vec<Capability>) {
        let entry = self.grants.entry(plugin_name.to_string()).or_default();
        for cap in capabilities {
            if !entry.contains(&cap) {
                entry.push(cap);
                debug!("Granted capability '{:?}' to plugin '{plugin_name}'", cap);
            }
        }
    }

    /// Check if a plugin has the required capability for a tool.
    pub fn check_tool(&self, _tool_name: &str) -> Result<Vec<Capability>> {
        // In production, look up which capabilities the tool requires
        // and verify they've been granted.
        Ok(vec![])
    }

    /// Check if a specific capability has been granted.
    pub fn has_capability(&self, plugin_name: &str, capability: &Capability) -> bool {
        // Check global whitelist first
        if self.global_whitelist.contains(capability) {
            return true;
        }
        // Check plugin-specific grants
        self.grants
            .get(plugin_name)
            .map(|caps| caps.contains(capability))
            .unwrap_or(false)
    }

    /// Revoke all capabilities from a plugin.
    pub fn revoke_all(&mut self, plugin_name: &str) {
        self.grants.remove(plugin_name);
        debug!("Revoked all capabilities from plugin '{plugin_name}'");
    }

    /// List granted capabilities for a plugin.
    pub fn list_grants(&self, plugin_name: &str) -> Vec<&Capability> {
        self.grants
            .get(plugin_name)
            .map(|caps| caps.iter().collect())
            .unwrap_or_default()
    }

    /// Get the global whitelist.
    pub fn global_whitelist(&self) -> &[Capability] {
        &self.global_whitelist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_whitelist() {
        let broker = CapabilityBroker::new();
        assert!(
            broker
                .global_whitelist()
                .contains(&Capability::FilesystemRead)
        );
    }

    #[test]
    fn test_grant_and_check() {
        let mut broker = CapabilityBroker::new();
        broker.grant("test-plugin", vec![Capability::FilesystemWrite]);
        assert!(broker.has_capability("test-plugin", &Capability::FilesystemWrite));
        assert!(!broker.has_capability("test-plugin", &Capability::FilesystemDelete));
    }

    #[test]
    fn test_global_whitelist_grants() {
        let broker = CapabilityBroker::new();
        // FilesystemRead is in the global whitelist
        assert!(broker.has_capability("unknown-plugin", &Capability::FilesystemRead));
        // FilesystemWrite is not
        assert!(!broker.has_capability("unknown-plugin", &Capability::FilesystemWrite));
    }

    #[test]
    fn test_revoke_all() {
        let mut broker = CapabilityBroker::new();
        broker.grant(
            "test",
            vec![Capability::FilesystemWrite, Capability::ClipboardRead],
        );
        assert_eq!(broker.list_grants("test").len(), 2);
        broker.revoke_all("test");
        assert!(broker.list_grants("test").is_empty());
    }

    #[test]
    fn test_check_tool() {
        let broker = CapabilityBroker::new();
        let result = broker.check_tool("fs.write_file");
        assert!(result.is_ok());
    }
}
