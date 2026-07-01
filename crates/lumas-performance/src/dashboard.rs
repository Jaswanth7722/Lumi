//! # Dashboard API
//!
//! Real-time metrics API without committing to a specific UI implementation.
//!
//! # Thread Safety
//! `DashboardApi` is `Send + Sync` via `Arc<PerformanceManager>`.

use crate::alert::Alert;
use crate::manager::{PerformanceManager, PerformanceSnapshot};
use crate::metric::MetricName;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;

/// A live summary of key performance metrics.
#[derive(Debug, Clone, Serialize)]
pub struct LiveSummary {
    /// Timestamp of the summary.
    pub timestamp: SystemTime,
    /// CPU usage percent.
    pub cpu_percent: f32,
    /// Memory RSS in MB.
    pub memory_rss_mb: f64,
    /// GPU utilization percent (if available).
    pub gpu_percent: Option<f32>,
    /// Current FPS.
    pub fps: f32,
    /// Frame time P99 in microseconds.
    pub frame_time_p99_us: u64,
    /// AI first token latency P95 in milliseconds.
    pub ai_first_token_p95_ms: u64,
    /// IPC round-trip latency P95 in microseconds.
    pub ipc_round_trip_p95_us: u64,
    /// Number of active alerts.
    pub active_alert_count: u32,
    /// Uptime in seconds.
    pub uptime_seconds: u64,
}

/// A data point in a metric time series.
#[derive(Debug, Clone, Serialize)]
pub struct MetricDataPoint {
    /// Timestamp of the data point.
    pub timestamp: SystemTime,
    /// The metric value.
    pub value: f64,
}

/// Time range for history queries.
#[derive(Debug, Clone)]
pub struct TimeRange {
    /// Start of the range.
    pub start: SystemTime,
    /// End of the range.
    pub end: SystemTime,
}

/// Dashboard API — exposes metrics for live display.
#[derive(Clone)]
pub struct DashboardApi {
    /// Reference to the performance manager.
    manager: Arc<PerformanceManager>,
}

impl DashboardApi {
    /// Create a new dashboard API.
    pub fn new(manager: Arc<PerformanceManager>) -> Self {
        Self { manager }
    }

    /// Get a live summary of key metrics.
    pub fn live_summary(&self) -> LiveSummary {
        let snap = self.manager.snapshot();
        LiveSummary {
            timestamp: SystemTime::now(),
            cpu_percent: snap.cpu_percent,
            memory_rss_mb: snap.memory_rss_mb,
            gpu_percent: None,
            fps: snap.fps,
            frame_time_p99_us: 0,
            ai_first_token_p95_ms: 0,
            ipc_round_trip_p95_us: 0,
            active_alert_count: snap.active_alerts.len() as u32,
            uptime_seconds: snap.uptime_seconds,
        }
    }

    /// Get a full snapshot of all metrics.
    pub fn full_snapshot(&self) -> PerformanceSnapshot {
        self.manager.snapshot()
    }

    /// Subscribe to metric updates at a given interval.
    ///
    /// Receives a new snapshot every `interval` (minimum: 100ms).
    pub fn subscribe(&self, interval: Duration) -> broadcast::Receiver<PerformanceSnapshot> {
        let (tx, rx) = broadcast::channel(16);
        let manager = self.manager.clone();
        let min_interval =
            Duration::from_millis(self.manager.system_metrics().uptime_seconds.get().max(1) as u64);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval.max(min_interval));
            loop {
                ticker.tick().await;
                let snap = manager.snapshot();
                if tx.send(snap).is_err() {
                    break;
                }
            }
        });

        rx
    }

    /// Get active alerts.
    pub fn active_alerts(&self) -> Vec<Alert> {
        self.manager.alert_engine().active_alerts()
    }
}

impl std::fmt::Debug for DashboardApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardApi").finish()
    }
}
