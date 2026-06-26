//! # Testing Utilities
//!
//! Test helpers for building and asserting state machine behavior.
//! This module is only available in test builds or with feature = "testing".

use crate::action::Action;
use crate::context::StateContext;
use crate::error::{EventId, MachineId, StateId, StateResult};
use crate::event::StateEvent;
use crate::guard::{Guard, GuardOutcome};
use crate::machine::StateMachine;
use crate::transition::TransitionDefinition;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Builds a simple state machine for testing purposes.
pub struct TestMachineBuilder {
    machine: StateMachine,
    next_transition_id: u32,
}

impl TestMachineBuilder {
    /// Create a new test machine builder.
    pub fn new(id: MachineId, name: &'static str) -> Self {
        Self {
            machine: StateMachine::new(id, name),
            next_transition_id: 1,
        }
    }

    /// Add a state.
    pub fn add_state(mut self, id: StateId, name: &'static str) -> Self {
        self.machine
            .add_state(Arc::new(crate::state::LeafState::new(
                id,
                name,
                self.machine.id,
            )));
        self
    }

    /// Set the initial state.
    pub fn initial_state(mut self, id: StateId) -> Self {
        self.machine.initial_state = id;
        self
    }

    /// Add a transition.
    pub fn add_transition(mut self, from: StateId, to: StateId, event: EventId) -> Self {
        let id = self.next_transition_id;
        self.next_transition_id += 1;
        self.machine
            .add_transition(TransitionDefinition::new(id, from, to, event));
        self
    }

    /// Build the state machine.
    pub fn build(self) -> StateMachine {
        self.machine
    }
}

/// A mock action that records invocations.
pub struct MockAction {
    /// Name of this action.
    pub name: &'static str,
    /// Number of times this action was called.
    pub call_count: Arc<AtomicUsize>,
    /// Whether this action should fail.
    pub fail: bool,
    /// Error message if failing.
    pub error_message: Option<String>,
}

impl MockAction {
    /// Create a new mock action.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            call_count: Arc::new(AtomicUsize::new(0)),
            fail: false,
            error_message: None,
        }
    }

    /// Get the call count.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for MockAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockAction")
            .field("name", &self.name)
            .field("call_count", &self.call_count.load(Ordering::Relaxed))
            .field("fail", &self.fail)
            .finish()
    }
}

#[async_trait]
impl Action for MockAction {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn execute(&self, _ctx: &mut StateContext, _event: &StateEvent) -> StateResult<()> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            Err(crate::error::StateError::Internal(
                self.error_message
                    .clone()
                    .unwrap_or_else(|| "mock action failed".into()),
            ))
        } else {
            Ok(())
        }
    }
}

/// A mock guard with configurable outcomes.
pub struct MockGuard {
    /// Name of this guard.
    pub name: &'static str,
    /// Whether to allow or deny.
    pub should_allow: bool,
    /// Whether to return an error.
    pub should_error: bool,
    /// Denial reason.
    pub denial_reason: &'static str,
}

impl MockGuard {
    /// Create a new mock guard that allows.
    pub fn allow(name: &'static str) -> Self {
        Self {
            name,
            should_allow: true,
            should_error: false,
            denial_reason: "",
        }
    }

    /// Create a new mock guard that denies.
    pub fn deny(name: &'static str, reason: &'static str) -> Self {
        Self {
            name,
            should_allow: false,
            should_error: false,
            denial_reason: reason,
        }
    }

    /// Create a new mock guard that errors.
    pub fn error(name: &'static str) -> Self {
        Self {
            name,
            should_allow: false,
            should_error: true,
            denial_reason: "guard error",
        }
    }
}

impl std::fmt::Debug for MockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockGuard")
            .field("name", &self.name)
            .field("should_allow", &self.should_allow)
            .finish()
    }
}

#[async_trait]
impl Guard for MockGuard {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn evaluate(
        &self,
        _ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, crate::error::GuardError> {
        if self.should_error {
            Err(crate::error::GuardError {
                guard_name: self.name,
                message: "mock guard error".into(),
            })
        } else if self.should_allow {
            Ok(GuardOutcome::Allow)
        } else {
            Ok(GuardOutcome::Deny {
                reason: self.denial_reason.into(),
            })
        }
    }
}

/// A fake scheduler that advances time manually.
pub struct FakeScheduler {
    /// Current fake time.
    pub current_time: std::time::Instant,
}

impl FakeScheduler {
    /// Create a new fake scheduler.
    pub fn new() -> Self {
        Self {
            current_time: std::time::Instant::now(),
        }
    }

    /// Advance time by the given duration.
    pub fn advance(&mut self, duration: std::time::Duration) {
        self.current_time += duration;
    }
}

impl Default for FakeScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Assert that a machine is in the expected state.
#[macro_export]
macro_rules! assert_state {
    ($machine:expr, $expected:expr) => {
        assert_eq!(
            $machine.current_state(),
            $expected,
            "Expected machine to be in state {:?}",
            $expected
        );
    };
}

/// Assert a transition outcome.
#[macro_export]
macro_rules! assert_transition_outcome {
    ($outcome:expr, completed) => {
        match $outcome {
            $crate::transition::TransitionOutcome::Completed { .. } => {}
            _ => panic!("Expected Completed, got {:?}", $outcome),
        }
    };
    ($outcome:expr, rejected) => {
        match $outcome {
            $crate::transition::TransitionOutcome::Rejected { .. } => {}
            _ => panic!("Expected Rejected, got {:?}", $outcome),
        }
    };
    ($outcome:expr, rolled_back) => {
        match $outcome {
            $crate::transition::TransitionOutcome::RolledBack { .. } => {}
            _ => panic!("Expected RolledBack, got {:?}", $outcome),
        }
    };
}

/// Fire an event and assert the resulting state.
/// This is a compile-time test helper for synchronous contexts.
#[macro_export]
macro_rules! fire_and_assert {
    ($manager:expr, $machine_id:expr, $event:expr, $expected_state:expr) => {{
        let outcome = $manager
            .send_and_wait($machine_id, $event, std::time::Duration::from_secs(5))
            .await
            .expect("send_and_wait failed");
        assert_transition_outcome!(outcome, completed);
        let snapshot = $manager
            .current_state($machine_id)
            .expect("current_state failed");
        assert_eq!(snapshot.state_id, $expected_state);
    }};
}
