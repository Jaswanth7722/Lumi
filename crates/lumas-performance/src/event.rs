//! # Event Types
//!
//! Event types emitted by the performance monitoring system for IPC bus integration.
//!
//! # Thread Safety
//! All event types implement `Clone + Send + Sync`.

use crate::alert::{Alert, AlertEvent};
use crate::threshold::ThresholdId;
use serde::Serialize;
use std::time::SystemTime;

/// Performance event emitted to the event bus.
#[derive(Debug, Clone, Serialize)]
pub enum PerformanceEvent {
    /// A threshold was breached.
    ThresholdBreached {
        /// Threshold ID.
        threshold_id: String,
        /// The metric value that breached.
        metric_value: f64,
        /// When it happened.
        timestamp: SystemTime,
    },
    /// An alert was fired.
    AlertFired {
        /// The alert.
        alert: Alert,
    },
    /// An alert was resolved.
    AlertResolved {
        /// Alert ID.
        alert_id: String,
        /// Timestamp.
        timestamp: SystemTime,
    },
    /// Metrics snapshot exported.
    MetricsExported {
        /// Export path.
        path: String,
        /// Number of metrics.
        metric_count: usize,
        /// Timestamp.
        timestamp: SystemTime,
    },
    /// Collector interval completed.
    CollectorTick {
        /// Collector ID.
        collector_id: String,
        /// Duration of the tick.
        duration_ms: u64,
        /// Timestamp.
        timestamp: SystemTime,
    },
    /// Performance anomaly detected.
    AnomalyDetected {
        /// Metric name.
        metric: String,
        /// Baseline P99 value.
        baseline_p99: u64,
        /// Observed value.
        observed_value: u64,
        /// Deviation factor.
        deviation_factor: f32,
        /// Timestamp.
        timestamp: SystemTime,
    },
}

impl std::fmt::Display for PerformanceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerformanceEvent::ThresholdBreached { threshold_id, .. } => {
                write!(f, "ThresholdBreached({})", threshold_id)
            }
            PerformanceEvent::AlertFired { alert } => {
                write!(f, "AlertFired({})", alert.id)
            }
            PerformanceEvent::AlertResolved { alert_id, .. } => {
                write!(f, "AlertResolved({})", alert_id)
            }
            PerformanceEvent::MetricsExported { path, .. } => {
                write!(f, "MetricsExported({})", path)
            }
            PerformanceEvent::CollectorTick { collector_id, .. } => {
                write!(f, "CollectorTick({})", collector_id)
            }
            PerformanceEvent::AnomalyDetected { metric, .. } => {
                write!(f, "AnomalyDetected({})", metric)
            }
        }
    }
}
