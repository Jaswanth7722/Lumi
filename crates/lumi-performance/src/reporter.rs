//! # Performance Reporter
//!
//! Generates performance reports from snapshots for diagnostics, exports,
//! and the dashboard API.

use crate::manager::PerformanceSnapshot;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// A performance report for export or diagnostic display.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceReport {
    /// When the report was generated.
    pub generated_at: SystemTime,
    /// Timestamp as Unix seconds.
    pub timestamp_secs: u64,
    /// The snapshot this report is based on.
    pub snapshot: PerformanceSnapshot,
    /// Summary statistics.
    pub summary: ReportSummary,
}

/// Summary statistics for a performance report.
#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    /// Total number of metrics tracked.
    pub total_metrics: usize,
    /// Number of counter metrics.
    pub counter_count: usize,
    /// Number of gauge metrics.
    pub gauge_count: usize,
    /// Number of histogram metrics.
    pub histogram_count: usize,
    /// Number of active alerts.
    pub active_alerts: u32,
    /// System CPU usage percent.
    pub cpu_percent: f32,
    /// System memory RSS in MB.
    pub memory_rss_mb: f64,
    /// Current FPS if available.
    pub fps: f32,
    /// Uptime in seconds.
    pub uptime_seconds: u64,
}

impl PerformanceReport {
    /// Create a new performance report from a snapshot.
    pub fn from_snapshot(snapshot: &PerformanceSnapshot) -> Self {
        let counter_count = snapshot
            .metrics
            .iter()
            .filter(|m| matches!(m.kind, crate::metric::MetricKind::Counter))
            .count();
        let gauge_count = snapshot
            .metrics
            .iter()
            .filter(|m| matches!(m.kind, crate::metric::MetricKind::Gauge))
            .count();
        let histogram_count = snapshot
            .metrics
            .iter()
            .filter(|m| m.histogram.is_some())
            .count();

        Self {
            generated_at: SystemTime::now(),
            timestamp_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            snapshot: snapshot.clone(),
            summary: ReportSummary {
                total_metrics: snapshot.metrics.len(),
                counter_count,
                gauge_count,
                histogram_count,
                active_alerts: snapshot.active_alerts.len() as u32,
                cpu_percent: snapshot.cpu_percent,
                memory_rss_mb: snapshot.memory_rss_mb,
                fps: snapshot.fps,
                uptime_seconds: snapshot.uptime_seconds,
            },
        }
    }

    /// Export the report as JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
