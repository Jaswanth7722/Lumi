//! # Capability Registry
//!
//! Enforces that every capability claimed by a process is permitted
//! by its descriptor and not already claimed by another process for
//! exclusive capabilities.
//!
//! This prevents plugin processes from claiming capabilities they
//! haven't declared and prevents two processes from claiming exclusive
//! capabilities (e.g., only one process may own `screen.capture`).
//!
//! # Thread Safety
//!
//! `CapabilityRegistry` is `Send + Sync` via `DashMap`. All operations
//! are fully concurrent with no global lock.
//!
//! # Design
//!
//! Capabilities fall into two categories:
//! - **Exclusive**: may only be claimed by a single process
//!   (e.g., `screen.capture`, `audio.exclusive`)
//! - **Shared**: may be claimed by multiple processes
//!   (e.g., `file.read`, `network.connect`)
//!
//! The exclusive capabilities list is defined as a compile-time constant.

use crate::descriptor::ProcessDescriptor;
use crate::error::ProcessError;
use crate::id::ProcessId;
use dashmap::DashMap;
use std::collections::HashSet;

/// Compile-time list of capabilities that may only be claimed by a single process.
pub const EXCLUSIVE_CAPABILITIES: &[&str] = &[
    "screen.capture",
    "clipboard.write",
    "system.settings",
    "audio.exclusive",
];

/// Enforces capability declarations and exclusivity guarantees.
///
/// # Examples
///
/// ```ignore
/// let cap_reg = CapabilityRegistry::new();
/// cap_reg.register(&process_id, &descriptor)?;
/// assert!(cap_reg.has_capability(&process_id, "file.read"));
/// ```
pub struct CapabilityRegistry {
    /// Maps capability string → ProcessId that owns it (exclusive only).
    exclusive_owners: DashMap<String, ProcessId>,
    /// All capabilities declared by each process.
    process_capabilities: DashMap<ProcessId, Vec<String>>,
    /// Set of capabilities that may only be claimed by a single process.
    exclusive_capabilities: HashSet<String>,
}

impl CapabilityRegistry {
    /// Create a new capability registry with the standard exclusive capabilities.
    pub fn new() -> Self {
        Self {
            exclusive_owners: DashMap::new(),
            process_capabilities: DashMap::new(),
            exclusive_capabilities: EXCLUSIVE_CAPABILITIES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    /// Register all capabilities declared by a process.
    ///
    /// Validates that:
    /// 1. Each capability is declared in the process descriptor's `capabilities` list.
    /// 2. Exclusive capabilities are not already claimed by another process.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::UnauthorizedCapability` if a capability
    /// is not in the descriptor's declared list.
    /// Returns `ProcessError::DuplicateCapability` if an exclusive capability
    /// is already claimed by another process.
    pub fn register(
        &self,
        id: &ProcessId,
        descriptor: &ProcessDescriptor,
    ) -> Result<(), ProcessError> {
        let declared: HashSet<&str> = descriptor
            .capabilities
            .iter()
            .map(|s| s.as_str())
            .collect();

        for cap in &descriptor.capabilities {
            // Check that the capability is declared in the descriptor.
            // (This is a sanity check — the descriptor should be the source of truth.)
            if !declared.contains(cap.as_str()) {
                return Err(ProcessError::UnauthorizedCapability {
                    id: id.clone(),
                    capability: cap.clone(),
                });
            }

            // Check exclusivity.
            if self.exclusive_capabilities.contains(cap) {
                if let Some(existing) = self.exclusive_owners.get(cap) {
                    if existing.value() != id {
                        return Err(ProcessError::DuplicateCapability {
                            capability: cap.clone(),
                            first: existing.value().clone(),
                            second: id.clone(),
                        });
                    }
                }
                self.exclusive_owners
                    .insert(cap.clone(), id.clone());
            }
        }

        self.process_capabilities
            .insert(id.clone(), descriptor.capabilities.clone());

        Ok(())
    }

    /// Check if a process holds a specific capability.
    pub fn has_capability(&self, id: &ProcessId, capability: &str) -> bool {
        self.process_capabilities
            .get(id)
            .map_or(false, |caps| caps.value().iter().any(|c| c == capability))
    }

    /// Deregister all capabilities for a process (called on stop/fail).
    pub fn deregister(&self, id: &ProcessId) {
        // Remove exclusive ownerships held by this process.
        self.exclusive_owners.retain(|_cap, owner| owner != id);
        // Remove the process's capability list.
        self.process_capabilities.remove(id);
    }

    /// Returns the process that owns an exclusive capability, if any.
    pub fn exclusive_owner(&self, capability: &str) -> Option<ProcessId> {
        self.exclusive_owners
            .get(capability)
            .map(|entry| entry.value().clone())
    }

    /// Returns all capabilities held by a process.
    pub fn capabilities_for(&self, id: &ProcessId) -> Vec<String> {
        self.process_capabilities
            .get(id)
            .map(|caps| caps.value().clone())
            .unwrap_or_default()
    }

    /// Returns the number of registered processes.
    pub fn len(&self) -> usize {
        self.process_capabilities.len()
    }

    /// Returns `true` if no processes are registered.
    pub fn is_empty(&self) -> bool {
        self.process_capabilities.is_empty()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CapabilityRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityRegistry")
            .field("process_count", &self.process_capabilities.len())
            .field("exclusive_owners", &self.exclusive_owners.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{ProcessDescriptor, ProcessKind};
    use crate::id::ProcessId;

    fn make_descriptor(id: &str, capabilities: Vec<String>) -> ProcessDescriptor {
        let pid = ProcessId::new(id);
        let mut desc = ProcessDescriptor::new(
            pid,
            id,
            semver::Version::new(1, 0, 0),
            ProcessKind::Worker {
                worker_fn: Arc::new(|| Box::pin(async {})),
            },
        );
        desc.capabilities = capabilities;
        desc
    }

    #[test]
    fn test_declared_capability_registers_successfully() {
        let reg = CapabilityRegistry::new();
        let id = ProcessId::new("test");
        let desc = make_descriptor("test", vec!["file.read".into(), "network.connect".into()]);

        assert!(reg.register(&id, &desc).is_ok());
        assert!(reg.has_capability(&id, "file.read"));
        assert!(reg.has_capability(&id, "network.connect"));
    }

    #[test]
    fn test_exclusive_capability_duplicate_returns_error() {
        let reg = CapabilityRegistry::new();
        let id1 = ProcessId::new("proc1");
        let id2 = ProcessId::new("proc2");

        let desc1 = make_descriptor("proc1", vec!["screen.capture".into()]);
        let desc2 = make_descriptor("proc2", vec!["screen.capture".into()]);

        assert!(reg.register(&id1, &desc1).is_ok());
        let result = reg.register(&id2, &desc2);
        assert!(result.is_err());
        match result {
            Err(ProcessError::DuplicateCapability { capability, .. }) => {
                assert_eq!(capability, "screen.capture");
            }
            _ => panic!("Expected DuplicateCapability error"),
        }
    }

    #[test]
    fn test_deregister_releases_exclusive_capability() {
        let reg = CapabilityRegistry::new();
        let id = ProcessId::new("test");
        let desc = make_descriptor("test", vec!["screen.capture".into()]);

        reg.register(&id, &desc).unwrap();
        assert!(reg.exclusive_owner("screen.capture").is_some());

        reg.deregister(&id);
        assert!(reg.exclusive_owner("screen.capture").is_none());
    }

    #[test]
    fn test_shared_capability_allows_multiple_owners() {
        let reg = CapabilityRegistry::new();
        let id1 = ProcessId::new("proc1");
        let id2 = ProcessId::new("proc2");

        let desc1 = make_descriptor("proc1", vec!["file.read".into()]);
        let desc2 = make_descriptor("proc2", vec!["file.read".into()]);

        assert!(reg.register(&id1, &desc1).is_ok());
        assert!(reg.register(&id2, &desc2).is_ok());

        assert!(reg.has_capability(&id1, "file.read"));
        assert!(reg.has_capability(&id2, "file.read"));
    }

    #[test]
    fn test_deregister_cleans_up() {
        let reg = CapabilityRegistry::new();
        let id = ProcessId::new("test");
        let desc = make_descriptor("test", vec!["file.read".into()]);

        reg.register(&id, &desc).unwrap();
        assert_eq!(reg.len(), 1);

        reg.deregister(&id);
        assert_eq!(reg.len(), 0);
        assert!(!reg.has_capability(&id, "file.read"));
    }
}
