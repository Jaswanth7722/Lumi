//! # State Machine Metrics
//!
//! Metrics for tracking state machine performance and behavior.
//! Integrates with the performance monitoring system.

use crate::error::{MachineId, StateId};
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Manager-level metrics for all state machines.
#[derive(Debug)]
pub struct ManagerMetrics {
    /// Total transitions processed.
    pub total_transitions: AtomicU64,
    /// Successful transitions.
    pub successful_transitions: AtomicU64,
    /// Rejected transitions.
    pub rejected_transitions: AtomicU64,
    /// Rolled back transitions.
    pub rolled_back_transitions: AtomicU64,
    /// Total time spent in transitions.
    pub total_transition_time_us: AtomicU64,
    /// Per-machine state residency tracking.
    pub state_residency: DashMap<(MachineId, StateId), StateResidency>,
}

impl ManagerMetrics {
    /// Create new manager metrics.
    pub fn new() -> Self {
        Self {
            total_transitions: AtomicU64::new(0),
            successful_transitions: AtomicU64::new(0),
            rejected_transitions: AtomicU64::new(0),
            rolled_back_transitions: AtomicU64::new(0),
            total_transition_time_us: AtomicU64::new(0),
            state_residency: DashMap::new(),
        }
    }

    /// Record a successful transition.
    pub fn record_transition(&self, duration: Duration) {
        self.total_transitions.fetch_add(1, Ordering::Relaxed);
        self.successful_transitions.fetch_add(1, Ordering::Relaxed);
        self.total_transition_time_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// Record a rejected transition.
    pub fn record_rejection(&self) {
        self.total_transitions.fetch_add(1, Ordering::Relaxed);
        self.rejected_transitions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a rolled back transition.
    pub fn record_rollback(&self) {
        self.total_transitions.fetch_add(1, Ordering::Relaxed);
        self.rolled_back_transitions.fetch_add(1, Ordering::Relaxed);
    }

    /// Enter a state (for residency tracking).
    pub fn enter_state(&self, machine: MachineId, state: StateId) {
        self.state_residency
            .entry((machine, state))
            .or_insert_with(|| StateResidency {
                machine_id: machine,
                state_id: state,
                total_entries: AtomicU64::new(0),
                total_residency_us: AtomicU64::new(0),
                last_entry: Instant::now(),
            })
            .total_entries
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Exit a state, recording residency.
    pub fn exit_state(&self, machine: MachineId, state: StateId) {
        if let Some(entry) = self.state_residency.get(&(machine, state)) {
            let elapsed = entry.last_entry.elapsed().as_micros() as u64;
            entry
                .total_residency_us
                .fetch_add(elapsed, Ordering::Relaxed);
        }
    }

    /// Get rejection rate (0.0 to 1.0).
    pub fn rejection_rate(&self) -> f64 {
        let total = self.total_transitions.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        self.rejected_transitions.load(Ordering::Relaxed) as f64 / total as f64
    }

    /// Get average transition duration in microseconds.
    pub fn avg_transition_duration_us(&self) -> f64 {
        let total = self.successful_transitions.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        self.total_transition_time_us.load(Ordering::Relaxed) as f64 / total as f64
    }
}

impl Default for ManagerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Residency tracking for a single state.
#[derive(Debug)]
pub struct StateResidency {
    /// Machine this state belongs to.
    pub machine_id: MachineId,
    /// State ID.
    pub state_id: StateId,
    /// Number of times this state has been entered.
    pub total_entries: AtomicU64,
    /// Total time spent in this state.
    pub total_residency_us: AtomicU64,
    /// When this state was last entered.
    pub last_entry: Instant,
}
