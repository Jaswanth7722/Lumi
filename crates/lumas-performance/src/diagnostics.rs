//! # Performance Diagnostics
//!
//! Provides performance anomaly detection and diagnostic report generation
//! that integrates with lumi-error's crash report system.
//!
//! # Thread Safety
//! All types are `Send + Sync`.

use crate::manager::PerformanceSnapshot;
use crate::metric::MetricName;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// A performance anomaly detected by the system.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceAnomaly {
    /// The affected metric.
    pub metric: String,
    /// When the anomaly was detected.
    pub detected_at: SystemTime,
    /// Human-readable description.
    pub description: String,
    /// Baseline P99 value.
    pub baseline_p99: u64,
    /// Observed value.
    pub observed_value: u64,
    /// Deviation factor (e.g., 3.2x above baseline).
    pub deviation_factor: f32,
}

/// A performance recommendation.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceRecommendation {
    /// Priority (1 = highest).
    pub priority: u8,
    /// Description of the recommendation.
    pub description: String,
    /// Affected metric name.
    pub affected_metric: String,
    /// Suggested configuration change.
    pub suggested_config_change: Option<String>,
}

/// Summary of a threshold violation.
#[derive(Debug, Clone, Serialize)]
pub struct ThresholdViolationSummary {
    /// Threshold ID.
    pub threshold_id: String,
    /// Metric name.
    pub metric: String,
    /// Severity.
    pub severity: String,
    /// Current value.
    pub value: f64,
    /// Threshold value.
    pub threshold: f64,
}

/// Performance diagnostics report for integration with crash reporting.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceDiagnosticsReport {
    /// When the report was generated.
    pub generated_at: SystemTime,
    /// Full performance snapshot.
    pub snapshot: PerformanceSnapshot,
    /// Threshold violations.
    pub threshold_violations: Vec<ThresholdViolationSummary>,
    /// Detected anomalies.
    pub anomalies: Vec<PerformanceAnomaly>,
    /// Recommended actions.
    pub recommendations: Vec<PerformanceRecommendation>,
}

/// Rolling baseline tracker for anomaly detection.
#[derive(Debug)]
pub struct RollingBaseline {
    /// Historical P99 values by metric name.
    history: parking_lot::Mutex<HashMap<String, Vec<u64>>>,
    /// Maximum history entries per metric.
    max_entries: usize,
}

impl RollingBaseline {
    /// Create a new rolling baseline tracker.
    pub fn new(max_entries: usize) -> Self {
        Self {
            history: parking_lot::Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    /// Record an observed P99 value for a metric.
    pub fn record(&self, metric: &str, p99: u64) {
        let mut history = self.history.lock();
        let entries = history.entry(metric.to_string()).or_insert_with(Vec::new);
        entries.push(p99);
        while entries.len() > self.max_entries {
            entries.remove(0);
        }
    }

    /// Get the baseline (median) P99 for a metric.
    pub fn baseline_p99(&self, metric: &str) -> Option<u64> {
        let history = self.history.lock();
        let entries = history.get(metric)?;
        if entries.is_empty() {
            return None;
        }
        let mut sorted = entries.clone();
        sorted.sort_unstable();
        Some(sorted[sorted.len() / 2])
    }
}

/// Detect anomalies by comparing current values to baseline.
pub fn detect_anomalies(
    baseline: &RollingBaseline,
    snapshot: &PerformanceSnapshot,
    deviation_threshold: f64,
) -> Vec<PerformanceAnomaly> {
    let mut anomalies = Vec::new();

    for metric in &snapshot.metrics {
        if let Some(ref hist) = metric.histogram {
            let p99 = hist.p99;
            if let Some(baseline_p99) = baseline.baseline_p99(&metric.name) {
                if baseline_p99 > 0 {
                    let deviation = p99 as f64 / baseline_p99 as f64;
                    if deviation > deviation_threshold {
                        anomalies.push(PerformanceAnomaly {
                            metric: metric.name.clone(),
                            detected_at: SystemTime::now(),
                            description: format!(
                                "P99 {} is {:.1}x above baseline P99 {}",
                                p99, deviation, baseline_p99
                            ),
                            baseline_p99,
                            observed_value: p99,
                            deviation_factor: deviation as f32,
                        });
                    }
                }
            }
        }
    }

    anomalies
}
