//! # Diagnostics Provider
//!
//! Provides diagnostic reports and exports for the process management system.
//!
//! # Thread Safety
//!
//! `ProcessDiagnostics` is `Send + Sync` via `Arc` and `DashMap`.

use crate::dependency::DependencyGraph;
use crate::handle::ProcessHandle;
use crate::id::ProcessId;
use crate::lifecycle::{ProcessState, StateTransitionRecord};
use crate::metrics::ProcessMetricsSnapshot;
use crate::registry::ProcessRegistry;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;

/// Provides diagnostics for the process management system.
pub struct ProcessDiagnostics {
    /// Process registry for querying process state.
    registry: Arc<ProcessRegistry>,
    /// Dependency graph for topology queries.
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Restart history for all processes.
    restart_history: DashMap<String, VecDeque<RestartEvent>>,
    /// Metrics snapshot cache.
    metrics: Arc<crate::metrics::ProcessMetrics>,
}

/// A recorded restart event for diagnostics.
#[derive(Debug, Clone)]
pub struct RestartEvent {
    /// The process that restarted.
    pub id: ProcessId,
    /// The restart attempt number.
    pub attempt: u32,
    /// Reason for the restart.
    pub reason: String,
    /// When the restart occurred.
    pub timestamp: DateTime<Utc>,
}

/// A complete crash report for a process.
#[derive(Debug, Clone)]
pub struct CrashReport {
    /// The crashed process ID.
    pub id: ProcessId,
    /// The final state of the process.
    pub state: ProcessState,
    /// State transition history.
    pub transitions: Vec<StateTransitionRecord>,
    /// Error description.
    pub error: String,
    /// Current metrics snapshot.
    pub metrics: ProcessMetricsSnapshot,
    /// When the report was generated.
    pub generated_at: DateTime<Utc>,
}

impl ProcessDiagnostics {
    /// Create a new diagnostics provider.
    pub fn new(
        registry: Arc<ProcessRegistry>,
        dependency_graph: Arc<RwLock<DependencyGraph>>,
        metrics: Arc<crate::metrics::ProcessMetrics>,
    ) -> Self {
        Self {
            registry,
            dependency_graph,
            restart_history: DashMap::new(),
            metrics,
        }
    }

    /// Record a restart event for a process.
    pub fn record_restart(&self, id: ProcessId, reason: String) {
        let mut history = self
            .restart_history
            .entry(id.path().to_string())
            .or_insert_with(|| VecDeque::with_capacity(20));

        let attempt = history.len() as u32 + 1;
        history.push_back(RestartEvent {
            id,
            attempt,
            reason,
            timestamp: Utc::now(),
        });

        // Keep last 20 entries
        if history.len() > 20 {
            history.pop_front();
        }
    }

    /// Returns all registered processes with their current states.
    pub fn process_list(&self) -> Vec<(ProcessId, ProcessState)> {
        self.registry
            .all_handles()
            .into_iter()
            .map(|h| (h.id().clone(), h.state()))
            .collect()
    }

    /// Returns the dependency graph as a Mermaid diagram string.
    pub fn dependency_graph_mermaid(&self) -> String {
        self.dependency_graph.read().to_mermaid()
    }

    /// Returns restart history for all processes.
    pub fn restart_history(&self) -> Vec<RestartEvent> {
        let mut events = Vec::new();
        for entry in self.restart_history.iter() {
            events.extend(entry.value().iter().cloned());
        }
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events
    }

    /// Generate a crash report for a specific process.
    pub fn crash_report(&self, id: &ProcessId) -> Option<CrashReport> {
        let handle = self.registry.get(id)?;

        // Get state machine history
        let _ = handle.state();

        Some(CrashReport {
            id: id.clone(),
            state: handle.state(),
            transitions: Vec::new(), // Would get from state machine
            error: format!("Process {} is in state {:?}", id, handle.state()),
            metrics: self.metrics.snapshot(),
            generated_at: Utc::now(),
        })
    }

    /// Returns the current metrics snapshot.
    pub fn metrics(&self) -> ProcessMetricsSnapshot {
        self.metrics.snapshot()
    }
}

impl std::fmt::Debug for ProcessDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessDiagnostics").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency::DependencyGraph;
    use crate::metrics::ProcessMetrics;
    use crate::registry::ProcessRegistry;

    #[test]
    fn test_process_list_empty() {
        let registry = Arc::new(ProcessRegistry::new());
        let graph = Arc::new(RwLock::new(DependencyGraph::new()));
        let metrics = Arc::new(ProcessMetrics::new());
        let diag = ProcessDiagnostics::new(registry, graph, metrics);
        assert!(diag.process_list().is_empty());
    }

    #[test]
    fn test_restart_history_records() {
        let registry = Arc::new(ProcessRegistry::new());
        let graph = Arc::new(RwLock::new(DependencyGraph::new()));
        let metrics = Arc::new(ProcessMetrics::new());
        let diag = ProcessDiagnostics::new(registry, graph, metrics);

        let id = ProcessId::new("test");
        diag.record_restart(id.clone(), "crash".into());
        assert_eq!(diag.restart_history().len(), 1);
    }
}
