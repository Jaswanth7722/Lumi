//! # State Machine Error Types
//!
//! All state machine errors are defined here. Errors integrate with `lumi-error`
//! for reporting and diagnostics.

use std::time::Duration;

/// Result alias for state operations.
pub type StateResult<T> = Result<T, StateError>;

/// Top-level state machine error type.
#[derive(Debug, Clone)]
pub enum StateError {
    /// Transition validation failed (guard rejected).
    TransitionRejected {
        transition_id: TransitionId,
        reason: String,
    },
    /// Exit action failed during transition.
    ExitActionFailed {
        transition_id: TransitionId,
        action_name: &'static str,
        cause: String,
    },
    /// Transition action failed.
    TransitionActionFailed {
        transition_id: TransitionId,
        action_name: &'static str,
        cause: String,
    },
    /// Entry action failed after commit.
    EntryActionFailed {
        transition_id: TransitionId,
        action_name: &'static str,
        cause: String,
    },
    /// Guard evaluation error.
    GuardError {
        guard_name: &'static str,
        cause: String,
    },
    /// Machine not found.
    MachineNotFound { machine_id: MachineId },
    /// Machine already registered.
    MachineAlreadyRegistered { machine_id: MachineId },
    /// State not found in machine.
    StateNotFound { state_id: StateId },
    /// Event not found in machine.
    EventNotFound { event_id: EventId },
    /// Invalid transition (no rule matches).
    InvalidTransition {
        source_state: StateId,
        event: EventId,
    },
    /// Concurrent transition contention (queue full).
    Contended {
        machine_id: MachineId,
        pending_count: usize,
    },
    /// Transition timed out.
    TransitionTimeout {
        transition_id: TransitionId,
        elapsed: Duration,
    },
    /// Guard evaluation timed out.
    GuardTimeout {
        guard_name: &'static str,
        elapsed: Duration,
    },
    /// Action timed out.
    ActionTimeout {
        action_name: &'static str,
        elapsed: Duration,
    },
    /// System is shutting down — cannot register machines or fire events.
    SystemShuttingDown,
    /// Event ID collision detected.
    EventIdCollision {
        id: u32,
        name_a: &'static str,
        name_b: &'static str,
    },
    /// State ID collision detected.
    StateIdCollision {
        id: u32,
        name_a: &'static str,
        name_b: &'static str,
    },
    /// History state unavailable.
    HistoryStateUnavailable { state_id: StateId },
    /// Internal error (wraps another error).
    Internal(String),
    /// Performance system error.
    PerformanceError(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::TransitionRejected {
                transition_id: tid,
                reason,
            } => {
                write!(f, "Transition {} rejected: {}", tid, reason)
            }
            StateError::ExitActionFailed {
                transition_id: tid,
                action_name,
                cause,
            } => {
                write!(
                    f,
                    "Exit action '{}' for transition {} failed: {}",
                    action_name, tid, cause
                )
            }
            StateError::TransitionActionFailed {
                transition_id: tid,
                action_name,
                cause,
            } => {
                write!(
                    f,
                    "Transition action '{}' for transition {} failed: {}",
                    action_name, tid, cause
                )
            }
            StateError::EntryActionFailed {
                transition_id: tid,
                action_name,
                cause,
            } => {
                write!(
                    f,
                    "Entry action '{}' for transition {} failed: {}",
                    action_name, tid, cause
                )
            }
            StateError::GuardError { guard_name, cause } => {
                write!(f, "Guard '{}' evaluation error: {}", guard_name, cause)
            }
            StateError::MachineNotFound { machine_id } => {
                write!(f, "Machine '{}' not found", machine_id)
            }
            StateError::MachineAlreadyRegistered { machine_id } => {
                write!(f, "Machine '{}' already registered", machine_id)
            }
            StateError::StateNotFound { state_id } => {
                write!(f, "State '{}' not found in machine", state_id)
            }
            StateError::EventNotFound { event_id } => {
                write!(f, "Event '{}' not found in machine", event_id)
            }
            StateError::InvalidTransition {
                source_state,
                event,
            } => {
                write!(
                    f,
                    "No transition from state {} for event {}",
                    source_state, event
                )
            }
            StateError::Contended {
                machine_id,
                pending_count,
            } => {
                write!(
                    f,
                    "Machine '{}' transition queue full ({} pending)",
                    machine_id, pending_count
                )
            }
            StateError::TransitionTimeout {
                transition_id,
                elapsed,
            } => {
                write!(
                    f,
                    "Transition {} timed out after {:?}",
                    transition_id, elapsed
                )
            }
            StateError::GuardTimeout {
                guard_name,
                elapsed,
            } => {
                write!(f, "Guard '{}' timed out after {:?}", guard_name, elapsed)
            }
            StateError::ActionTimeout {
                action_name,
                elapsed,
            } => {
                write!(f, "Action '{}' timed out after {:?}", action_name, elapsed)
            }
            StateError::SystemShuttingDown => write!(f, "System is shutting down"),
            StateError::EventIdCollision { id, name_a, name_b } => {
                write!(
                    f,
                    "Event ID {} collision: '{}' and '{}'",
                    id, name_a, name_b
                )
            }
            StateError::StateIdCollision { id, name_a, name_b } => {
                write!(
                    f,
                    "State ID {} collision: '{}' and '{}'",
                    id, name_a, name_b
                )
            }
            StateError::HistoryStateUnavailable { state_id } => {
                write!(
                    f,
                    "History state {} is not available (machine may have been reset)",
                    state_id
                )
            }
            StateError::Internal(msg) => write!(f, "Internal error: {}", msg),
            StateError::PerformanceError(msg) => write!(f, "Performance error: {}", msg),
        }
    }
}

impl std::error::Error for StateError {}

impl From<std::convert::Infallible> for StateError {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!()
    }
}

// ---------------------------------------------------------------------------
// ID types
// ---------------------------------------------------------------------------

/// Stable identifier for a state machine instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MachineId(pub u32);

impl MachineId {
    /// Create a new machine ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// The runtime lifecycle machine ID.
    pub const RUNTIME: MachineId = MachineId(0);
    /// Character behavior machine ID.
    pub const CHARACTER: MachineId = MachineId(1);
    /// AI processing machine ID.
    pub const AI: MachineId = MachineId(2);
    /// Voice processing machine ID.
    pub const VOICE: MachineId = MachineId(3);
    /// Plugin lifecycle machine ID.
    pub const PLUGIN: MachineId = MachineId(4);
    /// Render state machine ID.
    pub const RENDER: MachineId = MachineId(5);
    /// Workspace machine ID.
    pub const WORKSPACE: MachineId = MachineId(6);
}

impl std::fmt::Display for MachineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Machine({})", self.0)
    }
}

/// Stable, versioned integer identifier for a state.
///
/// Must never change across releases (used in persisted history).
/// States are registered in an append-only registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateId(pub u32);

impl StateId {
    /// Create a new state ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for StateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "State({})", self.0)
    }
}

impl From<u32> for StateId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Stable, versioned integer identifier for a transition definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionId(pub u32);

impl std::fmt::Display for TransitionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transition({})", self.0)
    }
}

impl From<u32> for TransitionId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Correlation ID for tracing async event chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub u64);

impl CorrelationId {
    /// Create a new correlation ID.
    pub fn new() -> Self {
        Self(rand_core_compat())
    }

    /// Create from a raw value.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Corr({})", self.0)
    }
}

fn rand_core_compat() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    (nanos & 0xFFFF_FFFF_FFFF_FFFF) as u64
}

/// Session ID for grouping related transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

impl SessionId {
    /// Create a new session ID.
    pub fn new() -> Self {
        Self(rand_core_compat() ^ 0xA5A5A5A5A5A5A5A5)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session({})", self.0)
    }
}

/// Timer ID for scheduled events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub u64);

impl std::fmt::Display for TimerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Timer({})", self.0)
    }
}

impl From<u64> for TimerId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// Subsystem ID (string identifier for cross-machine coordination).
pub type SubsystemId = String;

/// Plugin ID for plugin-related events.
pub type PluginId = String;

/// Stable, versioned integer identifier for an event.
///
/// Must never change across releases. Events are registered in an
/// append-only registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(pub u32);

impl EventId {
    /// Create a new event ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Event({})", self.0)
    }
}

impl From<u32> for EventId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Error produced by guard evaluation.
#[derive(Debug, Clone)]
pub struct GuardError {
    /// The guard that failed.
    pub guard_name: &'static str,
    /// Error message.
    pub message: String,
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Guard '{}' error: {}", self.guard_name, self.message)
    }
}

impl std::error::Error for GuardError {}

impl From<GuardError> for StateError {
    fn from(e: GuardError) -> Self {
        StateError::GuardError {
            guard_name: e.guard_name,
            cause: e.message,
        }
    }
}
