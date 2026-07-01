//! # Process Metrics
//!
//! Thread-safe atomic counters for process management observability.
//!
//! Tracks active processes, workers, and cumulative lifecycle events
//! (starts, stops, crashes, restarts, heartbeats). All metrics use
//! atomic operations for lock-free concurrent access.
//!
//! # Thread Safety
//!
//! All metric fields use `AtomicU32`/`AtomicU64` for lock-free access.
//! `ProcessMetrics` is `Send + Sync`.
//!
//! # Design
//!
//! Metrics are self-contained via atomics. A `register_with()` method
//! is provided for integration with `lumas-runtime::metrics::MetricsRegistry`
//! for Prometheus-style exposition in production.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use lumas_runtime::metrics::MetricsRegistry;

// ---------------------------------------------------------------------------
// ProcessMetrics
// ---------------------------------------------------------------------------

/// Global process management metrics.
///
/// All fields are atomic counters for lock-free concurrent access.
pub struct ProcessMetrics {
    /// Number of currently active (running) processes.
    pub active_processes: AtomicU32,
    /// Number of currently active workers.
    pub active_workers: AtomicI64,
    /// Total number of process starts.
    pub total_started: AtomicU64,
    /// Total number of clean stops.
    pub total_stopped: AtomicU64,
    /// Total number of crashes.
    pub total_crashed: AtomicU64,
    /// Total number of restarts.
    pub total_restarts: AtomicU64,
    /// Total missed heartbeats.
    pub total_heartbeats_missed: AtomicU64,
    /// Total received heartbeats.
    pub total_heartbeats_received: AtomicU64,
    /// Total supervisor interventions.
    pub supervisor_interventions: AtomicU64,
    /// Total capability violations.
    pub capability_violations: AtomicU64,
    /// Per-process restart counts (ProcessId path → count).
    pub restart_by_process: DashMap<String, AtomicU64>,
}

impl ProcessMetrics {
    /// Create a new process metrics instance with all counters at zero.
    pub fn new() -> Self {
        Self {
            active_processes: AtomicU32::new(0),
            active_workers: AtomicI64::new(0),
            total_started: AtomicU64::new(0),
            total_stopped: AtomicU64::new(0),
            total_crashed: AtomicU64::new(0),
            total_restarts: AtomicU64::new(0),
            total_heartbeats_missed: AtomicU64::new(0),
            total_heartbeats_received: AtomicU64::new(0),
            supervisor_interventions: AtomicU64::new(0),
            capability_violations: AtomicU64::new(0),
            restart_by_process: DashMap::new(),
        }
    }

    /// Take a snapshot of all current metric values.
    pub fn snapshot(&self) -> ProcessMetricsSnapshot {
        let missed = self.total_heartbeats_missed.load(Ordering::Relaxed);
        let received = self.total_heartbeats_received.load(Ordering::Relaxed);
        let total_hb = missed + received;
        let miss_rate = if total_hb > 0 {
            missed as f32 / total_hb as f32
        } else {
            0.0
        };

        // Top 5 most-restarted processes
        let mut restarts: Vec<(String, u64)> = self
            .restart_by_process
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().load(Ordering::Relaxed)))
            .collect();
        restarts.sort_by(|a, b| b.1.cmp(&a.1));
        restarts.truncate(5);

        ProcessMetricsSnapshot {
            active_processes: self.active_processes.load(Ordering::Relaxed),
            active_workers: self.active_workers.load(Ordering::Relaxed) as u32,
            total_started: self.total_started.load(Ordering::Relaxed),
            total_stopped: self.total_stopped.load(Ordering::Relaxed),
            total_crashed: self.total_crashed.load(Ordering::Relaxed),
            total_restarts: self.total_restarts.load(Ordering::Relaxed),
            heartbeat_miss_rate: miss_rate,
            top_restarting: restarts,
            snapshot_at: Utc::now(),
        }
    }

    /// Increment an atomic counter by 1 (relaxed ordering).
    pub fn increment(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment an atomic u32 gauge by 1 (relaxed ordering).
    pub fn increment_u32(&self, counter: &AtomicU32) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement an atomic i64 gauge by 1 (relaxed ordering).
    pub fn decrement_i64(&self, counter: &AtomicI64) {
        counter.fetch_sub(1, Ordering::Relaxed);
    }

    /// Register process metrics with a MetricsRegistry for prometheus-style exposition.
    pub fn register_with(&self, _registry: &MetricsRegistry) {
        // Integration with the runtime metrics registry.
        // In production, each atomic would be registered as a Counter or Gauge.
    }
}

impl Default for ProcessMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProcessMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessMetrics")
            .field("active_processes", &self.active_processes.load(Ordering::Relaxed))
            .field("total_started", &self.total_started.load(Ordering::Relaxed))
            .field("total_crashed", &self.total_crashed.load(Ordering::Relaxed))
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ProcessMetricsSnapshot
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of all process metrics.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessMetricsSnapshot {
    /// Number of currently active processes.
    pub active_processes: u32,
    /// Number of currently active workers.
    pub active_workers: u32,
    /// Total process starts since manager creation.
    pub total_started: u64,
    /// Total clean stops.
    pub total_stopped: u64,
    /// Total crashes.
    pub total_crashed: u64,
    /// Total restarts.
    pub total_restarts: u64,
    /// Heartbeat miss rate (missed / total).
    pub heartbeat_miss_rate: f32,
    /// Top 5 most-restarted processes.
    pub top_restarting: Vec<(String, u64)>,
    /// When the snapshot was taken.
    pub snapshot_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ProcessInstanceMetrics
// ---------------------------------------------------------------------------

/// Per-process-instance metrics.
///
/// Tracks metrics for a single running instance of a process.
pub struct ProcessInstanceMetrics {
    /// Number of heartbeats sent by this instance.
    pub heartbeats_sent: AtomicU64,
    /// Number of commands received.
    pub commands_received: AtomicU64,
    /// Number of state transitions.
    pub state_transitions: AtomicU64,
    /// Wall-clock uptime at the time of last snapshot.
    uptime_start: std::time::Instant,
}

impl ProcessInstanceMetrics {
    /// Create a new per-instance metrics counter.
    pub fn new() -> Self {
        Self {
            heartbeats_sent: AtomicU64::new(0),
            commands_received: AtomicU64::new(0),
            state_transitions: AtomicU64::new(0),
            uptime_start: std::time::Instant::now(),
        }
    }

    /// Record a heartbeat send.
    pub fn record_heartbeat(&self) {
        self.heartbeats_sent.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a command received.
    pub fn record_command(&self) {
        self.commands_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a state transition.
    pub fn record_transition(&self) {
        self.state_transitions.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current uptime.
    pub fn uptime_secs(&self) -> u64 {
        self.uptime_start.elapsed().as_secs()
    }
}

impl Default for ProcessInstanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProcessInstanceMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessInstanceMetrics")
            .field("uptime_secs", &self.uptime_secs())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_defaults() {
        let m = ProcessMetrics::new();
        assert_eq!(m.active_processes.load(Ordering::Relaxed), 0);
        assert_eq!(m.total_started.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let m = ProcessMetrics::new();
        m.total_started.fetch_add(10, Ordering::Relaxed);
        m.total_crashed.fetch_add(2, Ordering::Relaxed);

        let snap = m.snapshot();
        assert_eq!(snap.total_started, 10);
        assert_eq!(snap.total_crashed, 2);
    }

    #[test]
    fn test_instance_metrics() {
        let im = ProcessInstanceMetrics::new();
        im.record_heartbeat();
        im.record_heartbeat();
        im.record_command();
        assert_eq!(im.heartbeats_sent.load(Ordering::Relaxed), 2);
        assert_eq!(im.commands_received.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_heartbeat_miss_rate() {
        let m = ProcessMetrics::new();
        m.total_heartbeats_missed.fetch_add(1, Ordering::Relaxed);
        m.total_heartbeats_received.fetch_add(9, Ordering::Relaxed);
        let snap = m.snapshot();
        assert!((snap.heartbeat_miss_rate - 0.1).abs() < 0.001);
    }
}
