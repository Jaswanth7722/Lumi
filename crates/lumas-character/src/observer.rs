//! # State Machine Observer
//!
//! Thin wrappers over `lumas_state`'s observer system for character-specific
//! state machine observation. The Character Engine observes `MachineId::CHARACTER`
//! to react to state transitions.
//!
//! # Authority
//! Character Engine — observing state machine transitions.
//!
//! # Does NOT
//! - Define or own the state machine hierarchy
//! - Modify state machine state directly

use lumas_state::observer::TransitionEvent;
use tokio::sync::broadcast;

/// Convenience wrapper for observing character state transitions.
#[derive(Debug)]
pub struct CharacterObserver {
    receiver: broadcast::Receiver<TransitionEvent>,
}

impl CharacterObserver {
    /// Subscribe to character machine transitions from the given broadcast receiver.
    pub fn new(receiver: broadcast::Receiver<TransitionEvent>) -> Self {
        Self { receiver }
    }

    /// Try to receive the next transition event (non-blocking).
    pub fn try_recv(&mut self) -> Result<TransitionEvent, broadcast::error::TryRecvError> {
        self.receiver.try_recv()
    }

    /// Receive the next transition event (blocking).
    pub async fn recv(&mut self) -> Result<TransitionEvent, broadcast::error::RecvError> {
        self.receiver.recv().await
    }

    /// Get a new receiver (cloned from the same subscription).
    pub fn receiver(&self) -> broadcast::Receiver<TransitionEvent> {
        self.receiver.resubscribe()
    }
}
