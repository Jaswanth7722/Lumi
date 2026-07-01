//! # State Machine Definition
//!
//! Defines the state machine structure, transition table, and resolved transitions.
//! A `StateMachine` is the complete definition of a machine's states, transitions,
//! and configuration.

use crate::error::{EventId, MachineId, StateId, StateResult, SubsystemId};
use crate::event::StateEvent;
use crate::state::{MachineState, StateContext, StateSnapshot};
use crate::transition::TransitionDefinition;
use indexmap::IndexMap;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// A complete state machine definition.
///
/// Each state machine has a unique ID, a set of states, and a transition table.
/// Machines are registered with the `StateMachineManager` and process events
/// one at a time (per-machine serialization).
#[derive(Debug, Clone)]
pub struct StateMachine {
    /// Machine ID (unique across all machines).
    pub id: MachineId,
    /// Human-readable machine name.
    pub name: &'static str,
    /// States (IndexMap preserves insertion order).
    pub states: IndexMap<StateId, Arc<dyn MachineState>>,
    /// Transition table.
    pub transitions: TransitionTable,
    /// Initial state ID.
    pub initial_state: StateId,
    /// Final states (no outgoing transitions allowed).
    pub final_states: BTreeSet<StateId>,
    /// History configuration.
    pub history_config: HistoryConfig,
    /// Context schema for validation.
    pub context_schema: ContextSchema,
    /// Owning subsystem.
    pub owner: SubsystemId,
    /// Whether this machine supports cross-machine coordination.
    pub supports_cross_machine: bool,
}

impl StateMachine {
    /// Create a new state machine.
    pub fn new(id: MachineId, name: &'static str) -> Self {
        Self {
            id,
            name,
            states: IndexMap::new(),
            transitions: TransitionTable::new(),
            initial_state: StateId(0),
            final_states: BTreeSet::new(),
            history_config: HistoryConfig::default(),
            context_schema: ContextSchema::default(),
            owner: "unknown".to_string(),
            supports_cross_machine: true,
        }
    }

    /// Add a state to this machine.
    pub fn add_state(&mut self, state: Arc<dyn MachineState>) {
        self.states.insert(state.id(), state);
    }

    /// Add a transition to this machine.
    pub fn add_transition(&mut self, transition: TransitionDefinition) {
        self.transitions.add(transition);
    }

    /// Set the initial state.
    pub fn with_initial(mut self, state: StateId) -> Self {
        self.initial_state = state;
        self
    }

    /// Set the owner subsystem.
    pub fn with_owner(mut self, owner: &str) -> Self {
        self.owner = owner.to_string();
        self
    }

    /// Set history configuration.
    pub fn with_history(mut self, config: HistoryConfig) -> Self {
        self.history_config = config;
        self
    }

    /// Add a final state.
    pub fn add_final_state(mut self, state: StateId) -> Self {
        self.final_states.insert(state);
        self
    }

    /// Get a state by ID.
    pub fn get_state(&self, id: StateId) -> Option<&Arc<dyn MachineState>> {
        self.states.get(&id)
    }

    /// Whether a state exists in this machine.
    pub fn has_state(&self, id: StateId) -> bool {
        self.states.contains_key(&id)
    }

    /// Whether a state is final.
    pub fn is_final(&self, id: StateId) -> bool {
        self.final_states.contains(&id)
    }

    /// Validate the machine definition.
    pub fn validate(&self) -> StateResult<()> {
        if self.states.is_empty() {
            return Err(crate::error::StateError::Internal(
                "Machine has no states".into(),
            ));
        }
        if !self.states.contains_key(&self.initial_state) {
            return Err(crate::error::StateError::StateNotFound {
                state_id: self.initial_state,
            });
        }
        // Verify all transition references are valid
        for (key, transitions) in self.transitions.inner.iter() {
            let (source, _event) = key;
            if !self.states.contains_key(source) {
                return Err(crate::error::StateError::StateNotFound { state_id: *source });
            }
            for t in transitions {
                if !self.states.contains_key(&t.target) {
                    return Err(crate::error::StateError::StateNotFound { state_id: t.target });
                }
            }
        }
        Ok(())
    }

    /// Build a state snapshot from a running machine instance.
    pub fn build_snapshot(&self, instance: &MachineInstance) -> StateSnapshot {
        StateSnapshot {
            machine_id: self.id,
            state_id: instance.current_state,
            state_name: self
                .states
                .get(&instance.current_state)
                .map(|s| s.name())
                .unwrap_or("unknown"),
            entered_at: instance.state_entered_at,
            transition_count: instance.transition_count,
            active_substates: instance.active_substates.clone(),
        }
    }
}

// =========================================================================
// Transition Table
// =========================================================================

/// A table of transitions indexed by (source_state_id, event_id).
#[derive(Debug, Clone)]
pub struct TransitionTable {
    /// Transitions indexed by (source_state_id, event_id) → Vec<TransitionDefinition>
    /// Vec allows multiple transitions with priority ordering.
    pub inner: HashMap<(StateId, EventId), Vec<TransitionDefinition>>,
}

impl TransitionTable {
    /// Create a new transition table.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Add a transition definition.
    pub fn add(&mut self, transition: TransitionDefinition) {
        let key = (transition.source, transition.trigger);
        self.inner.entry(key).or_default().push(transition);
        // Sort by priority descending so highest priority is first
        if let Some(transitions) = self.inner.get_mut(&key) {
            transitions.sort_by(|a, b| b.priority.cmp(&a.priority));
        }
    }

    /// Resolve the highest-priority valid transition for the given state + event.
    ///
    /// Returns `Ok(None)` if no matching transition exists.
    pub fn resolve(
        &self,
        source: StateId,
        event: EventId,
    ) -> StateResult<Option<&TransitionDefinition>> {
        let key = (source, event);
        match self.inner.get(&key) {
            Some(transitions) if !transitions.is_empty() => Ok(Some(&transitions[0])),
            _ => Ok(None),
        }
    }

    /// Resolve all transitions matching a state + event (for priority evaluation).
    pub fn resolve_all(&self, source: StateId, event: EventId) -> Vec<&TransitionDefinition> {
        let key = (source, event);
        self.inner
            .get(&key)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Whether this table has any transitions for the given state.
    pub fn has_transitions_for(&self, source: StateId) -> bool {
        self.inner.keys().any(|(s, _)| *s == source)
    }

    /// Number of transition entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for TransitionTable {
    fn default() -> Self {
        Self::new()
    }
}

// =========================================================================
// Supporting Types
// =========================================================================

/// History configuration for a state machine.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Whether to track history.
    pub enabled: bool,
    /// Maximum history depth per composite state.
    pub max_depth: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: 8,
        }
    }
}

/// Context schema for validating context extensions.
#[derive(Debug, Clone, Default)]
pub struct ContextSchema {
    /// Allowed extension type names.
    pub allowed_extensions: Vec<String>,
}

/// A running instance of a state machine.
///
/// This represents the live state of a machine at runtime.
#[derive(Debug)]
pub struct MachineInstance {
    /// Current state.
    pub current_state: StateId,
    /// Previous state.
    pub previous_state: Option<StateId>,
    /// When the current state was entered.
    pub state_entered_at: std::time::Instant,
    /// Total transitions performed.
    pub transition_count: u64,
    /// Active substates (for composite states).
    pub active_substates: Vec<StateId>,
    /// History stack for composite states (shallow/deep history).
    pub history: HistoryStack,
}

impl MachineInstance {
    /// Create a new machine instance at a given initial state.
    pub fn new(initial_state: StateId) -> Self {
        Self {
            current_state: initial_state,
            previous_state: None,
            state_entered_at: std::time::Instant::now(),
            transition_count: 0,
            active_substates: Vec::new(),
            history: HistoryStack::new(),
        }
    }
}

/// History stack for preserving state across composite state transitions.
#[derive(Debug, Clone)]
pub struct HistoryStack {
    /// Shallow history: last active direct substate per composite state.
    pub shallow: HashMap<StateId, StateId>,
    /// Deep history: last active substate at all depths per composite state.
    pub deep: HashMap<StateId, Vec<StateId>>,
}

impl HistoryStack {
    /// Create a new empty history stack.
    pub fn new() -> Self {
        Self {
            shallow: HashMap::new(),
            deep: HashMap::new(),
        }
    }

    /// Record that a substate was active within a composite state.
    pub fn record_shallow(&mut self, composite: StateId, substate: StateId) {
        self.shallow.insert(composite, substate);
    }

    /// Record deep history (all levels).
    pub fn record_deep(&mut self, composite: StateId, path: Vec<StateId>) {
        self.deep.insert(composite, path);
    }

    /// Get the last active substate for a composite state (shallow).
    pub fn get_shallow(&self, composite: StateId) -> Option<StateId> {
        self.shallow.get(&composite).copied()
    }

    /// Get the deep history path for a composite state.
    pub fn get_deep(&self, composite: StateId) -> Option<&[StateId]> {
        self.deep.get(&composite).map(|v| v.as_slice())
    }
}

impl Default for HistoryStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to create EventId from u32.
pub(crate) const fn const_event(id: u32) -> EventId {
    EventId(id)
}
