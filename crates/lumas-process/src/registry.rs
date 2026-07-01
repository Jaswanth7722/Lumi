//! # Process Registry
//!
//! Thread-safe, concurrent process store backed by `DashMap`.
//!
//! The registry is the authoritative source of truth for all managed
//! processes. It maps `ProcessId` → `ProcessHandle` and provides
//! O(1) lookups, iteration, and atomic insert/remove operations.
//!
//! # Thread Safety
//!
//! `ProcessRegistry` is `Send + Sync` and `Clone` (O(1) via `Arc`).
//! All operations are fully concurrent with no global lock.

use crate::handle::ProcessHandle;
use crate::id::ProcessId;
use crate::lifecycle::ProcessState;
use dashmap::DashMap;
use std::sync::Arc;

/// Thread-safe registry of all managed processes.
///
/// Provides O(1) insert, lookup, and remove operations. The registry
/// is the single source of truth for the set of live processes.
///
/// # Examples
///
/// ```ignore
/// let registry = ProcessRegistry::new();
/// registry.insert(handle.clone());
/// let found = registry.get(&pid);
/// ```
#[derive(Clone)]
pub struct ProcessRegistry {
    inner: Arc<DashMap<ProcessId, ProcessHandle>>,
}

impl ProcessRegistry {
    /// Create a new empty process registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Insert a process handle into the registry.
    ///
    /// Returns the old handle if one existed for the same `ProcessId`.
    pub fn insert(&self, handle: ProcessHandle) -> Option<ProcessHandle> {
        self.inner.insert(handle.id().clone(), handle)
    }

    /// Look up a process handle by ID.
    pub fn get(&self, id: &ProcessId) -> Option<ProcessHandle> {
        self.inner.get(id).map(|entry| entry.value().clone())
    }

    /// Remove a process from the registry.
    ///
    /// Returns the removed handle, if any.
    pub fn remove(&self, id: &ProcessId) -> Option<ProcessHandle> {
        self.inner.remove(id).map(|(_k, v)| v)
    }

    /// Check if a process is registered.
    pub fn contains(&self, id: &ProcessId) -> bool {
        self.inner.contains_key(id)
    }

    /// Returns the number of registered processes.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns a snapshot of all process IDs currently registered.
    pub fn all_ids(&self) -> Vec<ProcessId> {
        self.inner.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Returns a snapshot of all process handles.
    pub fn all_handles(&self) -> Vec<ProcessHandle> {
        self.inner
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Returns a map of process IDs to current states.
    pub fn all_states(&self) -> dashmap::DashMap<ProcessId, ProcessState> {
        let states = DashMap::with_capacity(self.inner.len());
        for entry in self.inner.iter() {
            states.insert(entry.key().clone(), entry.value().state());
        }
        states
    }

    /// Returns all handles for processes in a given state.
    pub fn find_by_state(&self, state: ProcessState) -> Vec<ProcessHandle> {
        self.inner
            .iter()
            .filter(|entry| entry.value().state() == state)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Remove all processes from the registry.
    pub fn clear(&self) {
        self.inner.clear();
    }
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProcessRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessRegistry")
            .field("count", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::ProcessDescriptor;
    use crate::handle::ProcessCommand;
    use crate::heartbeat::HeartbeatSignal;
    use crate::lifecycle::ProcessStateMachine;
    use crate::metrics::ProcessInstanceMetrics;
    use crossbeam_channel::bounded;
    use tokio::sync::mpsc;

    fn make_handle(id: &str) -> ProcessHandle {
        let pid = ProcessId::new(id);
        let descriptor = Arc::new(ProcessDescriptor::new(
            pid.clone(),
            id,
            semver::Version::new(1, 0, 0),
            crate::descriptor::ProcessKind::Worker {
                worker_fn: Arc::new(|| {
                    Box::pin(async {})
                }),
            },
        ));
        let (hb_tx, _) = bounded(16);
        let (cmd_tx, _) = mpsc::channel(16);

        ProcessHandle::new(
            pid,
            descriptor,
            ProcessStateMachine::new(),
            Arc::new(ProcessInstanceMetrics::new()),
            hb_tx,
            cmd_tx,
            None,
        )
    }

    #[test]
    fn test_insert_and_get() {
        let registry = ProcessRegistry::new();
        let handle = make_handle("test.process");
        let pid = handle.id().clone();

        registry.insert(handle);
        assert!(registry.contains(&pid));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_remove() {
        let registry = ProcessRegistry::new();
        let handle = make_handle("test.process");
        let pid = handle.id().clone();

        registry.insert(handle);
        let removed = registry.remove(&pid);
        assert!(removed.is_some());
        assert!(!registry.contains(&pid));
    }

    #[test]
    fn test_all_ids() {
        let registry = ProcessRegistry::new();
        registry.insert(make_handle("a"));
        registry.insert(make_handle("b"));

        let ids = registry.all_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_find_by_state() {
        let registry = ProcessRegistry::new();
        registry.insert(make_handle("test"));
        let handles = registry.find_by_state(ProcessState::Registered);
        assert_eq!(handles.len(), 1);
    }

    #[test]
    fn test_clear() {
        let registry = ProcessRegistry::new();
        registry.insert(make_handle("a"));
        registry.insert(make_handle("b"));
        registry.clear();
        assert_eq!(registry.len(), 0);
    }
}
