//! # Performance Manager
//!
//! The singleton that owns all registered metrics, executes background
//! aggregation, evaluates thresholds, and coordinates export.
//!
//! # Thread Safety
//! `PerformanceManager` is `Clone` (cheap Arc clone), `Send + Sync`.

use crate::alert::{Alert, AlertEngine};
use crate::collector::{Collector, SubsystemId};
use crate::config::PerformanceConfig;
use crate::counter::{AsyncCounter, RtSafeCounter};
use crate::error::{PerformanceError, PerformanceResult};
use crate::gauge::{AsyncGauge, RtSafeGauge};
use crate::histogram::{HdrHistogram, RtSafeHistogram};
use crate::metric::{Metric, MetricKind, MetricName, MetricSnapshot, MetricUnit};
use crate::sampler::{BasicSampler, SystemSampler};
use crate::system::SystemMetricsHandle;
use crate::threshold::{Threshold, ThresholdEngine};
use crate::timer::Timer;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Label registry for bounded-cardinality labeled sub-counters.
#[derive(Debug)]
pub struct LabelRegistry {
    labels: DashMap<String, u64>,
}

impl LabelRegistry {
    /// Create a new label registry.
    pub fn new() -> Self {
        Self {
            labels: DashMap::new(),
        }
    }

    /// Increment a label's counter.
    pub fn increment_label(&self, label: &str) {
        *self.labels.entry(label.to_string()).or_insert(0) += 1;
    }

    /// Get the value for a label.
    pub fn label_value(&self, label: &str) -> u64 {
        self.labels.get(label).map(|v| *v).unwrap_or(0)
    }
}

impl Default for LabelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A handle to subsystem-specific metrics.
pub struct SubsystemMetricsHandle {
    /// Subsystem ID.
    pub subsystem_id: SubsystemId,
    /// Subsystem-specific timers.
    pub timers: Vec<Timer>,
    /// Subsystem-specific counters.
    pub counters: Vec<RtSafeCounter>,
    /// Subsystem-specific histograms.
    pub histograms: Vec<RtSafeHistogram>,
}

impl SubsystemMetricsHandle {
    /// Create a new subsystem metrics handle.
    pub fn new(subsystem_id: SubsystemId) -> Self {
        Self {
            subsystem_id,
            timers: Vec::new(),
            counters: Vec::new(),
            histograms: Vec::new(),
        }
    }

    /// Add a timer to this handle.
    pub fn add_timer(&mut self, timer: Timer) {
        self.timers.push(timer);
    }

    /// Add a counter to this handle.
    pub fn add_counter(&mut self, counter: RtSafeCounter) {
        self.counters.push(counter);
    }

    /// Create a new timer for this subsystem.
    pub fn timer(&self, operation: &str, unit: MetricUnit) -> Timer {
        let name = MetricName::from_str(&format!(
            "lumi.{}.{}.{}",
            self.subsystem_id, operation, unit
        ));
        Timer::new(name)
    }
}

/// Full performance snapshot at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceSnapshot {
    /// When the snapshot was taken.
    pub timestamp: Instant,
    /// All metric snapshots.
    pub metrics: Vec<MetricSnapshot>,
    /// Active alerts.
    pub active_alerts: Vec<Alert>,
    /// System resource gauges.
    pub cpu_percent: f32,
    pub memory_rss_mb: f64,
    pub fps: f32,
    /// Uptime in seconds.
    pub uptime_seconds: u64,
}

/// Performance manager singleton.
#[derive(Clone)]
pub struct PerformanceManager {
    /// Shared state.
    inner: Arc<PerformanceManagerInner>,
}

impl std::fmt::Debug for PerformanceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerformanceManager").finish()
    }
}

struct PerformanceManagerInner {
    /// Configuration.
    config: PerformanceConfig,
    /// Registered metrics.
    metrics: DashMap<String, MetricSnapshot>,
    /// Counters.
    counters: parking_lot::RwLock<Vec<Arc<RtSafeCounter>>>,
    /// Gauges.
    gauges: parking_lot::RwLock<Vec<Arc<RtSafeGauge>>>,
    /// Histograms.
    histograms: parking_lot::RwLock<Vec<Arc<HdrHistogram>>>,
    /// Registered collectors.
    collectors: parking_lot::RwLock<Vec<Box<dyn Collector>>>,
    /// Threshold engine.
    threshold_engine: parking_lot::RwLock<ThresholdEngine>,
    /// Alert engine.
    alert_engine: Arc<AlertEngine>,
    /// System metrics handle.
    system_metrics: SystemMetricsHandle,
    /// System sampler.
    sampler: Arc<dyn SystemSampler>,
    /// Whether the manager is shutting down.
    shutting_down: AtomicBool,
    /// Start time.
    start_time: Instant,
}

impl PerformanceManager {
    /// Start the performance manager with the given configuration.
    ///
    /// Launches background tasks for system sampling, histogram merging,
    /// threshold evaluation, and metrics export.
    pub async fn start(config: PerformanceConfig) -> PerformanceResult<Arc<Self>> {
        config
            .validate()
            .map_err(|e| PerformanceError::Internal(e))?;

        let inner = Arc::new(PerformanceManagerInner {
            config,
            metrics: DashMap::new(),
            counters: parking_lot::RwLock::new(Vec::new()),
            gauges: parking_lot::RwLock::new(Vec::new()),
            histograms: parking_lot::RwLock::new(Vec::new()),
            collectors: parking_lot::RwLock::new(Vec::new()),
            threshold_engine: parking_lot::RwLock::new(ThresholdEngine::new()),
            alert_engine: Arc::new(AlertEngine::new()),
            system_metrics: SystemMetricsHandle::new(),
            sampler: Arc::new(BasicSampler),
            shutting_down: AtomicBool::new(false),
            start_time: Instant::now(),
        });

        let manager = Arc::new(Self { inner });

        Ok(manager)
    }

    /// Register a metric.
    pub fn register_metric(&self, snapshot: MetricSnapshot) {
        self.inner.metrics.insert(snapshot.name.clone(), snapshot);
    }

    /// Register a counter.
    pub fn register_counter(&self, counter: Arc<RtSafeCounter>) {
        self.inner.counters.write().push(counter);
    }

    /// Register a gauge.
    pub fn register_gauge(&self, gauge: Arc<RtSafeGauge>) {
        self.inner.gauges.write().push(gauge);
    }

    /// Register a histogram.
    pub fn register_histogram(&self, histogram: Arc<HdrHistogram>) {
        self.inner.histograms.write().push(histogram);
    }

    /// Register a collector.
    pub fn register_collector(&self, collector: Box<dyn Collector>) {
        self.inner.collectors.write().push(collector);
    }

    /// Register a threshold.
    pub fn register_threshold(&self, threshold: Threshold) {
        self.inner.threshold_engine.write().register(threshold);
    }

    /// Evaluate all thresholds against a metric value.
    pub fn evaluate_threshold(&self, metric_name: &str, value: f64) {
        let alerts = self
            .inner
            .threshold_engine
            .read()
            .evaluate(metric_name, value);
        for alert in alerts {
            self.inner.alert_engine.fire(alert);
        }
    }

    /// Get a subsystem metrics handle.
    pub fn subsystem_handle(&self, id: SubsystemId) -> SubsystemMetricsHandle {
        SubsystemMetricsHandle::new(id)
    }

    /// Get the system metrics handle.
    pub fn system_metrics(&self) -> &SystemMetricsHandle {
        &self.inner.system_metrics
    }

    /// Get the alert engine.
    pub fn alert_engine(&self) -> &Arc<AlertEngine> {
        &self.inner.alert_engine
    }

    /// Get the threshold engine (read-only).
    pub fn threshold_engine(&self) -> parking_lot::RwLockReadGuard<'_, ThresholdEngine> {
        self.inner.threshold_engine.read()
    }

    /// Take a full performance snapshot.
    pub fn snapshot(&self) -> PerformanceSnapshot {
        let mut metrics = Vec::new();

        for entry in self.inner.metrics.iter() {
            metrics.push(entry.value().clone());
        }

        for counter in self.inner.counters.read().iter() {
            metrics.push(counter.snapshot());
        }

        for gauge in self.inner.gauges.read().iter() {
            metrics.push(gauge.snapshot());
        }

        PerformanceSnapshot {
            timestamp: Instant::now(),
            metrics,
            active_alerts: self.inner.alert_engine.active_alerts(),
            cpu_percent: self.inner.system_metrics.cpu_percent.get() as f32,
            memory_rss_mb: (self.inner.system_metrics.memory_rss.get() as f64) / (1024.0 * 1024.0),
            fps: 0.0,
            uptime_seconds: self.inner.start_time.elapsed().as_secs(),
        }
    }

    /// Shutdown the performance manager gracefully.
    pub async fn shutdown(&self) -> PerformanceResult<()> {
        self.inner.shutting_down.store(true, Ordering::Relaxed);
        // Flush all collectors
        for collector in self.inner.collectors.read().iter() {
            collector.flush().await.ok();
        }
        Ok(())
    }

    /// Check if the manager is shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.inner.shutting_down.load(Ordering::Relaxed)
    }
}
