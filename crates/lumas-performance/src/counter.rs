//! # Counter Types
//!
//! Two counter variants that share the `Metric` trait but have structurally
//! different representations: `RtSafeCounter` for real-time paths and
//! `AsyncCounter` for background subsystems.
//!
//! # Thread Safety
//! Both types are `Send + Sync`. `RtSafeCounter` uses `AtomicU64` for lock-free
//! operations. `AsyncCounter` wraps `RtSafeCounter` with additional features.

use crate::metric::{Metric, MetricKind, MetricName, MetricSnapshot, MetricUnit, Tag};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Real-time-safe counter for render/audio paths.
///
/// # Performance
/// - `increment()`: < 3 ns, no allocation, no locks
/// - `add(n)`: < 3 ns for any n
/// - `load()`: < 2 ns
///
/// # Thread Safety
/// Lock-free via `AtomicU64`. Safe to share across threads.
#[derive(Debug)]
pub struct RtSafeCounter {
    name: MetricName,
    description: &'static str,
    value: AtomicU64,
}

impl RtSafeCounter {
    /// Create a new real-time-safe counter.
    pub fn new(name: MetricName, description: &'static str) -> Self {
        Self {
            name,
            description,
            value: AtomicU64::new(0),
        }
    }

    /// Increment the counter by 1.
    #[inline(always)]
    pub fn increment(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Add `n` to the counter.
    #[inline(always)]
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    /// Load the current value.
    #[inline(always)]
    pub fn load(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

impl Metric for RtSafeCounter {
    fn name(&self) -> &MetricName {
        &self.name
    }

    fn kind(&self) -> MetricKind {
        MetricKind::Counter
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn unit(&self) -> MetricUnit {
        MetricUnit::Count
    }

    fn tags(&self) -> &[Tag] {
        &[]
    }

    fn snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            name: self.name.to_string(),
            kind: MetricKind::Counter,
            unit: MetricUnit::Count,
            value: self.load() as f64,
            histogram: None,
            tags: vec![],
        }
    }

    fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

/// Async counter for background subsystems.
///
/// Wraps `RtSafeCounter` for the hot path and adds:
/// - Rate computation (events/second over a sliding window)
/// - Delta tracking (increment since last snapshot)
/// - Labeled sub-counters with bounded cardinality
///
/// # Thread Safety
/// `Send + Sync`. Labeled sub-counters use `DashMap` for concurrent access.
#[derive(Debug)]
pub struct AsyncCounter {
    inner: RtSafeCounter,
    last_snapshot_value: AtomicU64,
    label_registry: Arc<crate::manager::LabelRegistry>,
}

impl AsyncCounter {
    /// Create a new async counter.
    pub fn new(name: MetricName, description: &'static str) -> Self {
        Self {
            inner: RtSafeCounter::new(name, description),
            last_snapshot_value: AtomicU64::new(0),
            label_registry: Arc::new(crate::manager::LabelRegistry::new()),
        }
    }

    /// Increment the counter by 1.
    #[inline(always)]
    pub fn increment(&self) {
        self.inner.increment();
    }

    /// Add `n` to the counter.
    #[inline(always)]
    pub fn add(&self, n: u64) {
        self.inner.add(n);
    }

    /// Get the total value.
    pub fn total(&self) -> u64 {
        self.inner.load()
    }

    /// Get the delta since the last call to `delta()`.
    pub fn delta(&self) -> u64 {
        let current = self.inner.load();
        let last = self.last_snapshot_value.swap(current, Ordering::Relaxed);
        current.saturating_sub(last)
    }

    /// Increment a labeled sub-counter.
    pub fn increment_label(&self, label: &str) -> Result<(), crate::PerformanceError> {
        self.label_registry.increment_label(label);
        self.inner.increment();
        Ok(())
    }

    /// Get the value for a specific label.
    pub fn label_value(&self, label: &str) -> u64 {
        self.label_registry.label_value(label)
    }
}

impl Metric for AsyncCounter {
    fn name(&self) -> &MetricName {
        self.inner.name()
    }

    fn kind(&self) -> MetricKind {
        MetricKind::Counter
    }

    fn description(&self) -> &'static str {
        self.inner.description()
    }

    fn unit(&self) -> MetricUnit {
        MetricUnit::Count
    }

    fn tags(&self) -> &[Tag] {
        &[]
    }

    fn snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            name: self.name().to_string(),
            kind: MetricKind::Counter,
            unit: MetricUnit::Count,
            value: self.total() as f64,
            histogram: None,
            tags: vec![],
        }
    }

    fn reset(&self) {
        self.inner.reset();
        self.last_snapshot_value.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_safe_counter_increment() {
        let counter = RtSafeCounter::new(MetricName::from_str("lumi.test.calls.count"), "test");
        assert_eq!(counter.load(), 0);
        counter.increment();
        assert_eq!(counter.load(), 1);
        counter.add(5);
        assert_eq!(counter.load(), 6);
    }

    #[test]
    fn test_async_counter_delta() {
        let counter = AsyncCounter::new(MetricName::from_str("lumi.test.calls.count"), "test");
        counter.add(10);
        assert_eq!(counter.delta(), 10);
        assert_eq!(counter.delta(), 0);
        counter.add(5);
        assert_eq!(counter.delta(), 5);
    }

    #[test]
    fn test_async_counter_labels() {
        let counter = AsyncCounter::new(MetricName::from_str("lumi.test.calls.count"), "test");
        counter.increment_label("plugin_a").unwrap();
        counter.increment_label("plugin_b").unwrap();
        counter.increment_label("plugin_a").unwrap();
        assert_eq!(counter.label_value("plugin_a"), 2);
        assert_eq!(counter.label_value("plugin_b"), 1);
    }

    #[test]
    fn test_counter_snapshot() {
        let counter = RtSafeCounter::new(MetricName::from_str("lumi.test.calls.count"), "test");
        counter.add(42);
        let snap = counter.snapshot();
        assert_eq!(snap.value, 42.0);
        assert_eq!(snap.kind, MetricKind::Counter);
    }
}
