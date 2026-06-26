//! # Diagnostics and Visualization
//!
//! Provides transition history storage, machine statistics, and graph
//! generation for debugging and visualization.

use crate::error::{MachineId, StateId};
use crate::manager::{PlatformStateSnapshot, StateMachineManager};
use crate::observer::TransitionEvent;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Diagnostics interface for the state machine system.
pub struct StateMachineDiagnostics {
    /// Reference to the manager.
    manager: Arc<StateMachineManager>,
    /// Transition history store.
    history: Arc<TransitionHistoryStore>,
}

impl StateMachineDiagnostics {
    /// Create a new diagnostics instance.
    pub fn new(manager: Arc<StateMachineManager>) -> Self {
        Self {
            history: Arc::new(TransitionHistoryStore::new(5000)),
            manager,
        }
    }

    /// Record a transition in the history store.
    pub fn record_transition(&self, event: TransitionEvent) {
        self.history.push(event);
    }

    /// Complete snapshot of all machine states.
    pub fn platform_snapshot(&self) -> PlatformStateSnapshot {
        self.manager.platform_snapshot()
    }

    /// Transition history for a machine within a time range.
    pub fn history(
        &self,
        machine_id: MachineId,
        since: SystemTime,
        limit: usize,
    ) -> Vec<TransitionEvent> {
        self.history.query(machine_id, since, limit)
    }

    /// Statistics per machine.
    pub fn statistics(&self, machine_id: MachineId) -> MachineStatistics {
        let events = self
            .history
            .query(machine_id, SystemTime::UNIX_EPOCH, usize::MAX);

        let total = events.len() as u64;
        let completed = events
            .iter()
            .filter(|e| matches!(e.outcome, crate::observer::TransitionOutcomeKind::Completed))
            .count() as u64;
        let rejected = events
            .iter()
            .filter(|e| {
                matches!(
                    e.outcome,
                    crate::observer::TransitionOutcomeKind::Rejected { .. }
                )
            })
            .count() as u64;
        let rolled_back = events
            .iter()
            .filter(|e| {
                matches!(
                    e.outcome,
                    crate::observer::TransitionOutcomeKind::RolledBack { .. }
                )
            })
            .count() as u64;

        let avg_duration_us = if completed > 0 {
            let total_us: u64 = events.iter().map(|e| e.duration_us).sum();
            total_us / completed.max(1)
        } else {
            0
        };

        MachineStatistics {
            machine_id,
            total_transitions: total,
            completed_transitions: completed,
            rejected_transitions: rejected,
            rolled_back_transitions: rolled_back,
            avg_duration_us,
        }
    }

    /// Generate a Mermaid state diagram.
    #[cfg(feature = "visualization")]
    pub fn to_mermaid(&self, machine_id: MachineId) -> Option<String> {
        let machine = self.manager.get_machine(machine_id)?;
        let mut mermaid = String::from("stateDiagram-v2\n");

        for (state_id, state) in &machine.states {
            let name = state.name().replace(|c: char| !c.is_alphanumeric(), "_");
            if state.is_final() {
                mermaid.push_str(&format!("    [*] --> {}\n", name));
                mermaid.push_str(&format!("    {} --> [*]\n", name));
            }
        }

        // Add transitions
        for (key, transitions) in &machine.transitions.inner {
            let (source, event_id) = key;
            if let Some(source_state) = machine.states.get(source) {
                let source_name = source_state
                    .name()
                    .replace(|c: char| !c.is_alphanumeric(), "_");
                for t in transitions {
                    if let Some(target_state) = machine.states.get(&t.target) {
                        let target_name = target_state
                            .name()
                            .replace(|c: char| !c.is_alphanumeric(), "_");
                        mermaid.push_str(&format!(
                            "    {} --> {} : Event({})\n",
                            source_name, target_name, event_id.0
                        ));
                    }
                }
            }
        }

        Some(mermaid)
    }

    /// Generate a DOT graph for Graphviz visualization.
    #[cfg(feature = "visualization")]
    pub fn to_dot(&self, machine_id: MachineId) -> Option<String> {
        let machine = self.manager.get_machine(machine_id)?;
        let mut dot = String::from("digraph StateMachine {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box, style=filled, fillcolor=lightblue];\n\n");

        for (state_id, state) in &machine.states {
            let name = state.name();
            if state.is_final() {
                dot.push_str(&format!("    {} [shape=doublecircle];\n", name));
            } else {
                dot.push_str(&format!("    {} [shape=box];\n", name));
            }
        }

        dot.push('\n');
        for (key, transitions) in &machine.transitions.inner {
            let (_source, event_id) = key;
            for t in transitions {
                let source_name = machine
                    .states
                    .get(&t.source)
                    .map(|s| s.name())
                    .unwrap_or("unknown");
                let target_name = machine
                    .states
                    .get(&t.target)
                    .map(|s| s.name())
                    .unwrap_or("unknown");
                dot.push_str(&format!(
                    "    {} -> {} [label=\"Evt{}\"];\n",
                    source_name, target_name, event_id.0
                ));
            }
        }

        dot.push_str("}\n");
        Some(dot)
    }
}

/// Statistics for a single machine.
#[derive(Debug, Clone)]
pub struct MachineStatistics {
    pub machine_id: MachineId,
    pub total_transitions: u64,
    pub completed_transitions: u64,
    pub rejected_transitions: u64,
    pub rolled_back_transitions: u64,
    pub avg_duration_us: u64,
}

/// Transition history store with bounded capacity.
pub struct TransitionHistoryStore {
    /// Stored events.
    events: parking_lot::Mutex<Vec<TransitionEvent>>,
    /// Maximum number of records to retain.
    max_records: usize,
}

impl TransitionHistoryStore {
    /// Create a new history store.
    pub fn new(max_records: usize) -> Self {
        Self {
            events: parking_lot::Mutex::new(Vec::with_capacity(max_records)),
            max_records,
        }
    }

    /// Push a transition event into the store.
    pub fn push(&self, event: TransitionEvent) {
        let mut events = self.events.lock();
        events.push(event);
        if events.len() > self.max_records {
            events.remove(0);
        }
    }

    /// Query transition history for a machine.
    pub fn query(
        &self,
        machine_id: MachineId,
        since: SystemTime,
        limit: usize,
    ) -> Vec<TransitionEvent> {
        let events = self.events.lock();
        events
            .iter()
            .filter(|e| e.machine_id == machine_id && e.timestamp >= since)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get all events for all machines.
    pub fn all(&self) -> Vec<TransitionEvent> {
        self.events.lock().clone()
    }

    /// Number of stored events.
    pub fn len(&self) -> usize {
        self.events.lock().len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.events.lock().is_empty()
    }
}
