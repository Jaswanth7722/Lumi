//! # Metrics Registry
//!
//! Thread-safe internal metrics registry for runtime observability.
//!
//! Supports counters (monotonically increasing), gauges (up/down),
//! histograms (sampled distributions), and timers (duration helpers).
//!
//! # Thread Safety
//!
//! All metric types use atomic operations internally and require no
//! external synchronization. The registry itself is backed by DashMap
//! for concurrent read/write access without a global lock.
//!
//! # Design
//!
//! Metrics are addressable by dotted name (e.g., `runtime.tasks.active`).
//! This naming scheme is designed for future Prometheus exposition.

use dashmap::DashMap;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Metric Value Types
// ---------------------------------------------------------------------------

/// A single metric snapshot value.
#[derive(Debug, Clone)]
pub enum MetricValue {
    /// A monotonically increasing counter value.
    Counter(u64),
    /// A gauge value that can go up or down.
    Gauge(i64),
    /// A histogram with bucket counts and aggregates.
    Histogram(HistogramValue),
    /// A timer measurement in milliseconds.
    Timer(f64),
}

impl fmt::Display for MetricValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricValue::Counter(v) => write!(f, "counter: {v}"),
            MetricValue::Gauge(v) => write!(f, "gauge: {v}"),
            MetricValue::Histogram(h) => write!(f, "histogram: {}", h.count),
            MetricValue::Timer(v) => write!(f, "timer: {v:.2}ms"),
        }
    }
}

/// Snapshot of histogram buckets and aggregates.
#[derive(Debug, Clone)]
pub struct HistogramValue {
    /// Number of samples recorded.
    pub count: u64,
    /// Sum of all sample values.
    pub sum: f64,
    /// Minimum observed value.
    pub min: f64,
    /// Maximum observed value.
    pub max: f64,
    /// Bucket boundaries and counts.
    pub buckets: Vec<(f64, u64)>,
}

// ---------------------------------------------------------------------------
// Metric Types
// ---------------------------------------------------------------------------

/// A monotonically increasing counter.
#[derive(Debug)]
pub struct Counter(AtomicU64);

impl Counter {
    /// Create a new counter starting at 0.
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Increment the counter by 1.
    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the counter by a given amount.
    pub fn add(&self, value: u64) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    /// Get the current value.
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// A gauge that can go up or down.
#[derive(Debug)]
pub struct Gauge(AtomicI64);

impl Gauge {
    /// Create a new gauge starting at 0.
    pub const fn new() -> Self {
        Self(AtomicI64::new(0))
    }

    /// Set the gauge to a specific value.
    pub fn set(&self, value: i64) {
        self.0.store(value, Ordering::Relaxed);
    }

    /// Increment the gauge by 1.
    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the gauge by 1.
    pub fn decrement(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }

    /// Add a delta to the gauge value.
    pub fn add(&self, delta: i64) {
        self.0.fetch_add(delta, Ordering::Relaxed);
    }

    /// Get the current value.
    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// A histogram for recording sampled value distributions.
#[derive(Debug)]
pub struct Histogram {
    /// Predefined bucket boundaries.
    buckets: Vec<f64>,
    /// Count per bucket (index + 1 for overflow).
    counts: Vec<AtomicU64>,
    /// Total count.
    total: AtomicU64,
    /// Sum of all values.
    sum: AtomicU64,
    /// Minimum value (stored as bits for atomic compare-exchange).
    min: AtomicU64,
    /// Maximum value.
    max: AtomicU64,
}

impl Histogram {
    /// Create a new histogram with the given bucket boundaries.
    ///
    /// The buckets define inclusive upper bounds. An extra bucket
    /// is implicitly created for values exceeding the last boundary.
    pub fn new(buckets: Vec<f64>) -> Self {
        let count = buckets.len() + 1; // +1 for overflow bucket
        Self {
            buckets,
            counts: (0..count).map(|_| AtomicU64::new(0)).collect(),
            total: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            min: AtomicU64::new(u64::MAX),
            max: AtomicU64::new(0),
        }
    }

    /// Create a histogram with default buckets (millisecond latency buckets).
    pub fn default_latency() -> Self {
        Self::new(vec![
            1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0,
        ])
    }

    /// Record a sample value.
    pub fn observe(&self, value: f64) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value as u64, Ordering::Relaxed);

        // Update min (atomic compare-exchange)
        let value_bits = value.to_bits();
        loop {
            let current = self.min.load(Ordering::Relaxed);
            if value_bits >= current {
                break;
            }
            if self
                .min
                .compare_exchange(current, value_bits, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        // Update max
        let value_bits = value.to_bits();
        loop {
            let current = self.max.load(Ordering::Relaxed);
            if value_bits <= current {
                break;
            }
            if self
                .max
                .compare_exchange(current, value_bits, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }

        // Find bucket
        let idx = self
            .buckets
            .iter()
            .position(|b| value <= *b)
            .unwrap_or(self.counts.len() - 1);
        self.counts[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of the histogram.
    pub fn snapshot(&self) -> HistogramValue {
        let count = self.total.load(Ordering::Relaxed);
        let sum = self.sum.load(Ordering::Relaxed) as f64;
        let min_bits = self.min.load(Ordering::Relaxed);
        let max_bits = self.max.load(Ordering::Relaxed);

        let min = if min_bits == u64::MAX {
            0.0
        } else {
            f64::from_bits(min_bits)
        };
        let max = if max_bits == 0 {
            0.0
        } else {
            f64::from_bits(max_bits)
        };

        let buckets: Vec<(f64, u64)> = self
            .buckets
            .iter()
            .zip(self.counts.iter())
            .map(|(bound, count)| (*bound, count.load(Ordering::Relaxed)))
            .collect();

        HistogramValue {
            count,
            sum,
            min,
            max,
            buckets,
        }
    }
}

/// A convenience wrapper around Histogram for timing measurements.
#[derive(Debug)]
pub struct Timer {
    histogram: Histogram,
}

impl Timer {
    /// Create a new timer with default latency buckets.
    pub fn new() -> Self {
        Self {
            histogram: Histogram::default_latency(),
        }
    }

    /// Record a duration in milliseconds.
    pub fn record_ms(&self, duration_ms: f64) {
        self.histogram.observe(duration_ms);
    }

    /// Get a snapshot of recorded timer values.
    pub fn snapshot(&self) -> HistogramValue {
        self.histogram.snapshot()
    }
}

// ---------------------------------------------------------------------------
// Metrics Registry
// ---------------------------------------------------------------------------

/// Thread-safe registry for runtime and subsystem metrics.
///
/// Metrics are addressable by dotted name (e.g., `runtime.tasks.active`).
/// The registry supports counters, gauges, histograms, and timers.
///
/// # Examples
///
/// ```ignore
/// let registry = MetricsRegistry::new();
/// let active_tasks = registry.gauge("runtime.tasks.active");
/// active_tasks.set(5);
/// ```
pub struct MetricsRegistry {
    /// Registered counters by name.
    counters: DashMap<String, Arc<Counter>>,
    /// Registered gauges by name.
    gauges: DashMap<String, Arc<Gauge>>,
    /// Registered histograms by name.
    histograms: DashMap<String, Arc<Histogram>>,
    /// Registered timers by name.
    timers: DashMap<String, Arc<Timer>>,
}

impl MetricsRegistry {
    /// Create a new empty metrics registry.
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
            gauges: DashMap::new(),
            histograms: DashMap::new(),
            timers: DashMap::new(),
        }
    }

    /// Register or retrieve a counter metric.
    pub fn counter(&self, name: &str) -> Arc<Counter> {
        self.counters
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Counter::new()))
            .value()
            .clone()
    }

    /// Register or retrieve a gauge metric.
    pub fn gauge(&self, name: &str) -> Arc<Gauge> {
        self.gauges
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Gauge::new()))
            .value()
            .clone()
    }

    /// Register or retrieve a histogram metric.
    pub fn histogram(&self, name: &str, buckets: Vec<f64>) -> Arc<Histogram> {
        self.histograms
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Histogram::new(buckets)))
            .value()
            .clone()
    }

    /// Register or retrieve a latency timer metric.
    pub fn timer(&self, name: &str) -> Arc<Timer> {
        self.timers
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Timer::new()))
            .value()
            .clone()
    }

    /// Export a snapshot of all registered metrics.
    pub fn snapshot(&self) -> HashMap<String, MetricValue> {
        let mut result = HashMap::new();

        for entry in self.counters.iter() {
            result.insert(entry.key().clone(), MetricValue::Counter(entry.get()));
        }

        for entry in self.gauges.iter() {
            result.insert(entry.key().clone(), MetricValue::Gauge(entry.get()));
        }

        for entry in self.histograms.iter() {
            result.insert(
                entry.key().clone(),
                MetricValue::Histogram(entry.snapshot()),
            );
        }

        for entry in self.timers.iter() {
            result.insert(entry.key().clone(), MetricValue::Timer(0.0));
        }

        result
    }

    /// Number of registered metrics.
    pub fn len(&self) -> usize {
        self.counters.len() + self.gauges.len() + self.histograms.len() + self.timers.len()
    }

    /// Whether any metrics are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new();
        assert_eq!(counter.get(), 0);
        counter.increment();
        assert_eq!(counter.get(), 1);
        counter.add(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new();
        gauge.set(42);
        assert_eq!(gauge.get(), 42);
        gauge.increment();
        assert_eq!(gauge.get(), 43);
        gauge.decrement();
        assert_eq!(gauge.get(), 42);
    }

    #[test]
    fn test_histogram() {
        let hist = Histogram::default_latency();
        hist.observe(10.0);
        hist.observe(50.0);
        hist.observe(200.0);

        let snap = hist.snapshot();
        assert_eq!(snap.count, 3);
        assert!(snap.sum >= 260.0);
    }

    #[test]
    fn test_registry() {
        let reg = MetricsRegistry::new();
        assert!(reg.is_empty());

        let c = reg.counter("test.counter");
        c.increment();

        assert_eq!(reg.len(), 1);

        let snap = reg.snapshot();
        assert!(snap.contains_key("test.counter"));
        match &snap["test.counter"] {
            MetricValue::Counter(v) => assert_eq!(*v, 1),
            _ => panic!("Expected counter"),
        }
    }
}
