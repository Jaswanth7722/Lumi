//! # Observer System
//!
//! Provides a subscription mechanism for observing state transitions.
//! `TransitionEvent`s are published to all subscribers and can be
//! consumed by downstream systems (render, workspace, performance).

use crate::error::{CorrelationId, EventId, MachineId, StateId};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;

/// A transition event published to observers.
#[derive(Debug, Clone)]
pub struct TransitionEvent {
    /// Machine that transitioned.
    pub machine_id: MachineId,
    /// Source state.
    pub from_state: StateId,
    /// Target state.
    pub to_state: StateId,
    /// Trigger event.
    pub trigger: EventId,
    /// Outcome kind.
    pub outcome: TransitionOutcomeKind,
    /// Duration of the transition.
    pub duration_us: u64,
    /// When the transition occurred.
    pub timestamp: SystemTime,
    /// Correlation ID for tracing.
    pub correlation_id: CorrelationId,
}

/// The kind of transition outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionOutcomeKind {
    /// Transition completed successfully.
    Completed,
    /// Transition was rejected by a guard.
    Rejected { guard: &'static str },
    /// Transition was rolled back.
    RolledBack { at_step: &'static str },
    /// Transition timed out.
    Timeout { elapsed_us: u64 },
}

/// A unique observer ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(pub u64);

/// Registry of transition observers.
///
/// Manages broadcast channels for per-machine and global observation.
/// Every transition event is published to both the per-machine channel
/// (if subscribed) and the global `observe_all` channel.
#[derive(Debug)]
pub struct ObserverRegistry {
    /// Global broadcast channel for all transitions.
    broadcast_all: broadcast::Sender<TransitionEvent>,
    /// Per-machine broadcast channels.
    per_machine: DashMap<MachineId, broadcast::Sender<TransitionEvent>>,
}

impl ObserverRegistry {
    /// Create a new observer registry.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            broadcast_all: tx,
            per_machine: DashMap::new(),
        }
    }

    /// Subscribe to all transitions across all machines.
    pub fn subscribe_all(&self) -> broadcast::Receiver<TransitionEvent> {
        self.broadcast_all.subscribe()
    }

    /// Subscribe to transitions on a specific machine.
    pub fn subscribe(&self, machine_id: MachineId) -> broadcast::Receiver<TransitionEvent> {
        let tx = self
            .per_machine
            .entry(machine_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .value()
            .clone();
        tx
    }

    /// Publish a transition event to all subscribers.
    ///
    /// This is best-effort: if a subscriber is lagging, the event is silently dropped.
    pub fn publish(&self, event: TransitionEvent) {
        let machine_id = event.machine_id;

        // Publish to global channel (best-effort)
        let _ = self.broadcast_all.send(event.clone());

        // Publish to per-machine channel (best-effort)
        if let Some(tx) = self.per_machine.get(&machine_id) {
            let _ = tx.send(event);
        }
    }

    /// Number of subscribers for a specific machine.
    pub fn subscriber_count(&self, machine_id: MachineId) -> usize {
        self.per_machine
            .get(&machine_id)
            .map(|tx| tx.receiver_count())
            .unwrap_or(0)
    }

    /// Total number of subscribers across all machines (including global).
    pub fn total_subscriber_count(&self) -> usize {
        let mut total = self.broadcast_all.receiver_count();
        for entry in self.per_machine.iter() {
            total += entry.receiver_count();
        }
        total
    }
}
