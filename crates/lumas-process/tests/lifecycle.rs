//! # Lifecycle Integration Tests
//!
//! Tests for the process state machine and lifecycle transitions.

use lumas_process::lifecycle::{ProcessState, ProcessStateMachine};
use lumas_process::error::ProcessError;

#[test]
fn test_registered_process_starts_correctly() {
    let mut sm = ProcessStateMachine::new();
    assert_eq!(sm.current(), ProcessState::Registered);
    assert!(sm.transition(ProcessState::Starting, "start").is_ok());
    assert_eq!(sm.current(), ProcessState::Starting);
}

#[test]
fn test_invalid_state_transition_returns_error() {
    let mut sm = ProcessStateMachine::new();
    // Can't go from Registered to Running directly
    let result = sm.transition(ProcessState::Running, "skip");
    assert!(result.is_err());
    match result {
        Err(ProcessError::InvalidStateTransition { from, to, .. }) => {
            assert_eq!(from, ProcessState::Registered);
            assert_eq!(to, ProcessState::Running);
        }
        _ => panic!("Expected InvalidStateTransition"),
    }
}

#[test]
fn test_process_reaches_ready_after_start() {
    let mut sm = ProcessStateMachine::new();
    sm.transition(ProcessState::Starting, "start").unwrap();
    sm.transition(ProcessState::Initializing, "init").unwrap();
    sm.transition(ProcessState::Ready, "ready").unwrap();
    assert_eq!(sm.current(), ProcessState::Ready);
    assert!(sm.current().is_operational());
}

#[test]
fn test_all_terminal_states_reject_further_transitions() {
    let mut sm = ProcessStateMachine::with_initial(ProcessState::Stopped);
    let result = sm.transition(ProcessState::Starting, "resurrect");
    assert!(result.is_err());

    let mut sm2 = ProcessStateMachine::with_initial(ProcessState::Failed);
    let result2 = sm2.transition(ProcessState::Starting, "resurrect");
    assert!(result2.is_err());
}
