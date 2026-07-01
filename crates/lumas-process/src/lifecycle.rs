//! # Process State Machine
//!
//! Typed state machine for managed process lifecycle.
//!
//! Every process managed by the supervision tree has a well-defined state
//! and a set of legal transitions. The `ProcessStateMachine` enforces
//! these transitions at runtime, records transition history for diagnostics,
//! and notifies subscribers on each transition.
//!
//! # Thread Safety
//!
//! `ProcessStateMachine` requires external synchronization via
//! `parking_lot::RwLock`. State reads are non-blocking. Transitions
//! should be performed while holding a write lock.
//!
//! # State Diagram
//!
//! ```text
//! Registered → Starting → Initializing → Ready → Running ⇄ Busy
//!                                                ↕        ↕
//!                                              Waiting → Ready
//!                                                ↕
//!                                              Paused → Running
//! Starting → Crashed → Restarting/Recovering → Starting/Failed
//! Ready/Running/Busy/Waiting → Stopping → Stopped
//! Crashed → Failed (max restarts exceeded)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tokio::sync::mpsc;

use crate::error::ProcessError;
use crate::id::ProcessId;

// ---------------------------------------------------------------------------
// ProcessState
// ---------------------------------------------------------------------------

/// All valid states for a managed process.
///
/// The state machine enforces valid transitions between these states.
/// Terminal states (`Stopped`, `Failed`) accept no further transitions.
///
/// # Thread Safety
///
/// `ProcessState` is `Copy` + `Send` + `Sync` and suitable for use
/// in atomic operations or as a map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProcessState {
    /// Descriptor accepted by the registry, not yet started.
    Registered,
    /// Launch initiated, awaiting first heartbeat or init completion.
    Starting,
    /// Process running, performing internal initialization (pre-Ready).
    Initializing,
    /// Initialized and accepting work.
    Ready,
    /// Actively processing work (as reported by heartbeat metadata).
    Running,
    /// Saturated; backpressure active.
    Busy,
    /// Blocked on dependency or external event.
    Waiting,
    /// Suspended by operator command.
    Paused,
    /// Supervisor-initiated restart in progress.
    Restarting,
    /// Crash detected; recovery strategy executing.
    Recovering,
    /// Graceful stop in progress.
    Stopping,
    /// Cleanly stopped (terminal).
    Stopped,
    /// Unrecoverable failure; max restarts exceeded (terminal).
    Failed,
    /// Unexpected termination; restart pending.
    Crashed,
}

impl ProcessState {
    /// Returns all valid target states from this state.
    pub fn valid_transitions(&self) -> &'static [ProcessState] {
        match self {
            ProcessState::Registered => &[ProcessState::Starting],
            ProcessState::Starting => &[ProcessState::Initializing, ProcessState::Crashed, ProcessState::Failed],
            ProcessState::Initializing => &[ProcessState::Ready, ProcessState::Crashed, ProcessState::Failed],
            ProcessState::Ready => &[
                ProcessState::Running,
                ProcessState::Busy,
                ProcessState::Waiting,
                ProcessState::Paused,
                ProcessState::Stopping,
                ProcessState::Crashed,
            ],
            ProcessState::Running => &[
                ProcessState::Ready,
                ProcessState::Busy,
                ProcessState::Waiting,
                ProcessState::Paused,
                ProcessState::Stopping,
                ProcessState::Crashed,
            ],
            ProcessState::Busy => &[
                ProcessState::Running,
                ProcessState::Ready,
                ProcessState::Waiting,
                ProcessState::Stopping,
                ProcessState::Crashed,
            ],
            ProcessState::Waiting => &[
                ProcessState::Running,
                ProcessState::Ready,
                ProcessState::Stopping,
                ProcessState::Crashed,
            ],
            ProcessState::Paused => &[ProcessState::Running, ProcessState::Stopping],
            ProcessState::Restarting => &[ProcessState::Starting, ProcessState::Failed],
            ProcessState::Recovering => &[ProcessState::Starting, ProcessState::Failed],
            ProcessState::Stopping => &[ProcessState::Stopped, ProcessState::Crashed],
            ProcessState::Stopped => &[],     // Terminal
            ProcessState::Failed => &[],       // Terminal
            ProcessState::Crashed => &[
                ProcessState::Restarting,
                ProcessState::Recovering,
                ProcessState::Failed,
            ],
        }
    }

    /// Returns `true` if this state represents a terminal condition.
    pub fn is_terminal(&self) -> bool {
        matches!(self, ProcessState::Stopped | ProcessState::Failed)
    }

    /// Returns `true` if the process is consuming resources (active).
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            ProcessState::Running
                | ProcessState::Busy
                | ProcessState::Waiting
                | ProcessState::Initializing
        )
    }

    /// Returns `true` if the process is running or can accept work.
    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            ProcessState::Ready | ProcessState::Running | ProcessState::Busy
        )
    }

    /// Returns `true` if the process has failed or crashed and needs recovery.
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            ProcessState::Crashed | ProcessState::Failed | ProcessState::Recovering
        )
    }
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessState::Registered => write!(f, "Registered"),
            ProcessState::Starting => write!(f, "Starting"),
            ProcessState::Initializing => write!(f, "Initializing"),
            ProcessState::Ready => write!(f, "Ready"),
            ProcessState::Running => write!(f, "Running"),
            ProcessState::Busy => write!(f, "Busy"),
            ProcessState::Waiting => write!(f, "Waiting"),
            ProcessState::Paused => write!(f, "Paused"),
            ProcessState::Restarting => write!(f, "Restarting"),
            ProcessState::Recovering => write!(f, "Recovering"),
            ProcessState::Stopping => write!(f, "Stopping"),
            ProcessState::Stopped => write!(f, "Stopped"),
            ProcessState::Failed => write!(f, "Failed"),
            ProcessState::Crashed => write!(f, "Crashed"),
        }
    }
}

// ---------------------------------------------------------------------------
// StateTransitionRecord
// ---------------------------------------------------------------------------

/// Records a single state transition for diagnostic purposes.
#[derive(Debug, Clone, Serialize)]
pub struct StateTransitionRecord {
    /// The source state.
    pub from: ProcessState,
    /// The target state.
    pub to: ProcessState,
    /// Human-readable reason for the transition.
    pub reason: String,
    /// When the transition occurred.
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ProcessStateMachine
// ---------------------------------------------------------------------------

/// The state machine for a single process. Enforces valid transitions,
/// records history, and notifies subscribers.
///
/// # Warning
///
/// Only one transition should be in-flight at a time. External
/// synchronization (e.g., `parking_lot::RwLock`) must be held
/// during `transition()` calls.
///
/// # Examples
///
/// ```ignore
/// let mut sm = ProcessStateMachine::new(ProcessState::Registered);
/// sm.transition(ProcessState::Starting, "bootstrap").unwrap();
/// assert_eq!(sm.current(), ProcessState::Starting);
/// ```
pub struct ProcessStateMachine {
    /// The current state.
    current: ProcessState,
    /// Transition history (most recent first, max 50 entries).
    history: VecDeque<StateTransitionRecord>,
    /// Subscribers notified on each transition.
    listeners: Vec<mpsc::Sender<StateTransitionRecord>>,
}

impl ProcessStateMachine {
    /// Create a new state machine starting at `ProcessState::Registered`.
    pub fn new() -> Self {
        Self {
            current: ProcessState::Registered,
            history: VecDeque::with_capacity(50),
            listeners: Vec::new(),
        }
    }

    /// Create a new state machine with a specific initial state.
    pub fn with_initial(state: ProcessState) -> Self {
        Self {
            current: state,
            history: VecDeque::with_capacity(50),
            listeners: Vec::new(),
        }
    }

    /// Attempt a transition to a new state.
    ///
    /// Returns `ProcessError::InvalidStateTransition` if the transition
    /// is not allowed by the state table.
    ///
    /// # Errors
    ///
    /// Returns `ProcessError::InvalidStateTransition` if the transition
    /// is not valid. The machine's state is unchanged.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub fn transition(
        &mut self,
        to: ProcessState,
        reason: impl Into<String>,
    ) -> Result<(), ProcessError> {
        let from = self.current;

        // Terminal states accept no transitions.
        if from.is_terminal() {
            return Err(ProcessError::InvalidStateTransition {
                id: ProcessId::root(), // Caller should override
                from,
                to,
            });
        }

        let allowed = from.valid_transitions();
        if !allowed.contains(&to) {
            return Err(ProcessError::InvalidStateTransition {
                id: ProcessId::root(), // Caller should override
                from,
                to,
            });
        }

        // Perform the transition.
        self.current = to;

        // Record history.
        let record = StateTransitionRecord {
            from,
            to,
            reason: reason.into(),
            timestamp: Utc::now(),
        };

        self.history.push_front(record.clone());
        if self.history.len() > 50 {
            self.history.pop_back();
        }

        // Notify subscribers (best-effort, ignore closed receivers).
        self.listeners.retain(|tx| {
            tx.try_send(record.clone()).is_ok()
        });

        Ok(())
    }

    /// Returns the current state.
    pub fn current(&self) -> ProcessState {
        self.current
    }

    /// Returns the transition history (most recent first).
    pub fn history(&self) -> &VecDeque<StateTransitionRecord> {
        &self.history
    }

    /// Subscribe to state transitions. The receiver is notified on
    /// every subsequent transition.
    ///
    /// Returns a `mpsc::Receiver` that yields `StateTransitionRecord` values.
    /// If the receiver is dropped, the subscription is silently removed
    /// on the next transition.
    pub fn subscribe(&mut self) -> mpsc::Receiver<StateTransitionRecord> {
        let (tx, rx) = mpsc::channel(64);
        self.listeners.push(tx);
        rx
    }

    /// Number of transitions recorded in history.
    pub fn transition_count(&self) -> usize {
        self.history.len()
    }
}

impl Default for ProcessStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProcessStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessStateMachine")
            .field("current", &self.current)
            .field("history_count", &self.history.len())
            .field("listeners", &self.listeners.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = ProcessStateMachine::new();
        assert_eq!(sm.current(), ProcessState::Registered);
    }

    #[test]
    fn test_valid_transition() {
        let mut sm = ProcessStateMachine::new();
        assert!(sm.transition(ProcessState::Starting, "start").is_ok());
        assert_eq!(sm.current(), ProcessState::Starting);
    }

    #[test]
    fn test_invalid_transition_returns_error() {
        let mut sm = ProcessStateMachine::new();
        // Can't go from Registered to Running directly.
        let result = sm.transition(ProcessState::Running, "skip");
        assert!(result.is_err());
        assert_eq!(sm.current(), ProcessState::Registered);
    }

    #[test]
    fn test_terminal_state_rejects_transitions() {
        let mut sm = ProcessStateMachine::with_initial(ProcessState::Stopped);
        let result = sm.transition(ProcessState::Starting, "resurrect");
        assert!(result.is_err());
        assert_eq!(sm.current(), ProcessState::Stopped);
    }

    #[test]
    fn test_history_is_recorded() {
        let mut sm = ProcessStateMachine::new();
        sm.transition(ProcessState::Starting, "bootstrap").unwrap();
        sm.transition(ProcessState::Initializing, "init").unwrap();
        sm.transition(ProcessState::Ready, "ready").unwrap();
        assert_eq!(sm.history().len(), 3);
    }

    #[test]
    fn test_full_active_machine() {
        let mut sm = ProcessStateMachine::new();
        sm.transition(ProcessState::Starting, "start").unwrap();
        assert!(sm.current().is_active());

        sm.transition(ProcessState::Initializing, "init").unwrap();
        assert!(sm.current().is_active());
    }

    #[test]
    fn test_displays() {
        assert_eq!(ProcessState::Registered.to_string(), "Registered");
        assert_eq!(ProcessState::Running.to_string(), "Running");
        assert_eq!(ProcessState::Stopped.to_string(), "Stopped");
        assert_eq!(ProcessState::Failed.to_string(), "Failed");
    }

    #[test]
    fn test_valid_transitions_registered() {
        let valid = ProcessState::Registered.valid_transitions();
        assert!(valid.contains(&ProcessState::Starting));
        assert_eq!(valid.len(), 1);
    }

    #[test]
    fn test_subscriber_notified() {
        let mut sm = ProcessStateMachine::new();
        let mut rx = sm.subscribe();

        sm.transition(ProcessState::Starting, "go").unwrap();

        let record = rx.try_recv().unwrap();
        assert_eq!(record.from, ProcessState::Registered);
        assert_eq!(record.to, ProcessState::Starting);
    }

    #[test]
    fn test_is_operational() {
        assert!(ProcessState::Ready.is_operational());
        assert!(ProcessState::Running.is_operational());
        assert!(ProcessState::Busy.is_operational());
        assert!(!ProcessState::Stopped.is_operational());
    }
}
