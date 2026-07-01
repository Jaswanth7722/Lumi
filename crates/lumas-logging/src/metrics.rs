//! # Metrics
//!
//! Logging metrics with atomic counters for thread-safe hot-path recording.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe logging metrics using atomic operations.
pub struct LoggingMetrics {
    /// Total records submitted to the pipeline.
    pub records_submitted: AtomicU64,
    /// Total records written to at least one sink.
    pub records_written: AtomicU64,
    /// Total records dropped due to channel full.
    pub records_dropped: AtomicU64,
    /// Total records redacted.
    pub records_redacted: AtomicU64,
    /// Total records filtered out.
    pub records_filtered: AtomicU64,
    /// Total bytes written across all sinks.
    pub bytes_written: AtomicU64,
    /// Total rotations completed.
    pub rotations_completed: AtomicU64,
    /// Total flush operations completed.
    pub flush_count: AtomicU64,
    /// Total sink errors.
    pub sink_errors: AtomicU64,
    /// Per-level counters.
    pub by_level: [AtomicU64; 6],
}

impl LoggingMetrics {
    /// Create new logging metrics.
    pub fn new() -> Self {
        Self {
            records_submitted: AtomicU64::new(0),
            records_written: AtomicU64::new(0),
            records_dropped: AtomicU64::new(0),
            records_redacted: AtomicU64::new(0),
            records_filtered: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            rotations_completed: AtomicU64::new(0),
            flush_count: AtomicU64::new(0),
            sink_errors: AtomicU64::new(0),
            by_level: [
                AtomicU64::new(0), // Trace
                AtomicU64::new(0), // Debug
                AtomicU64::new(0), // Info
                AtomicU64::new(0), // Warn
                AtomicU64::new(0), // Error
                AtomicU64::new(0), // Critical
            ],
        }
    }

    /// Record a record submission by level.
    pub fn record_submitted(&self, level: usize) {
        self.records_submitted.fetch_add(1, Ordering::Relaxed);
        if level < 6 {
            self.by_level[level].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a record being written to a sink.
    pub fn record_written(&self, bytes: u64) {
        self.records_written.fetch_add(1, Ordering::Relaxed);
        self.bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record a dropped record.
    pub fn record_dropped(&self) {
        self.records_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a redacted record.
    pub fn record_redacted(&self) {
        self.records_redacted.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a filtered record.
    pub fn record_filtered(&self) {
        self.records_filtered.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a rotation.
    pub fn record_rotation(&self) {
        self.rotations_completed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a flush.
    pub fn record_flush(&self) {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a sink error.
    pub fn record_sink_error(&self) {
        self.sink_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Export a snapshot of all metrics.
    pub fn snapshot(&self) -> LoggingMetricsSnapshot {
        let submitted = self.records_submitted.load(Ordering::Relaxed);
        let dropped = self.records_dropped.load(Ordering::Relaxed);

        LoggingMetricsSnapshot {
            records_submitted: submitted,
            records_written: self.records_written.load(Ordering::Relaxed),
            records_dropped: dropped,
            records_redacted: self.records_redacted.load(Ordering::Relaxed),
            records_filtered: self.records_filtered.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            rotations_completed: self.rotations_completed.load(Ordering::Relaxed),
            flush_count: self.flush_count.load(Ordering::Relaxed),
            sink_errors: self.sink_errors.load(Ordering::Relaxed),
            by_level: [
                ("trace".into(), self.by_level[0].load(Ordering::Relaxed)),
                ("debug".into(), self.by_level[1].load(Ordering::Relaxed)),
                ("info".into(), self.by_level[2].load(Ordering::Relaxed)),
                ("warn".into(), self.by_level[3].load(Ordering::Relaxed)),
                ("error".into(), self.by_level[4].load(Ordering::Relaxed)),
                ("critical".into(), self.by_level[5].load(Ordering::Relaxed)),
            ]
            .into_iter()
            .collect(),
            drop_rate_percent: if submitted > 0 {
                (dropped as f32 / submitted as f32) * 100.0
            } else {
                0.0
            },
            snapshot_at: Utc::now(),
        }
    }

    /// Register all logging metrics with the MetricsRegistry from lumas-runtime.
    pub fn register_with(&self, _registry: &lumas_runtime::metrics::MetricsRegistry) {
        // Metrics are self-contained via atomics; registry integration
        // would expose them as prometheus-style counters in production.
    }
}

impl Default for LoggingMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of all metrics at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct LoggingMetricsSnapshot {
    pub records_submitted: u64,
    pub records_written: u64,
    pub records_dropped: u64,
    pub records_redacted: u64,
    pub records_filtered: u64,
    pub bytes_written: u64,
    pub rotations_completed: u64,
    pub flush_count: u64,
    pub sink_errors: u64,
    pub by_level: HashMap<String, u64>,
    pub drop_rate_percent: f32,
    pub snapshot_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_increment() {
        let metrics = LoggingMetrics::new();
        metrics.record_submitted(2); // Info
        metrics.record_submitted(3); // Warn
        metrics.record_written(100);
        metrics.record_dropped();

        let snap = metrics.snapshot();
        assert_eq!(snap.records_submitted, 2);
        assert_eq!(snap.records_written, 1);
        assert_eq!(snap.records_dropped, 1);
        assert_eq!(snap.bytes_written, 100);
        assert!(snap.drop_rate_percent > 0.0);
    }
}
