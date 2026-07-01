//! # Character Metrics
//!
//! Integrates with `lumas-performance`'s subsystem metrics infrastructure.
//! Does not build a parallel metrics system.
//!
//! # Authority
//! Character Engine — character-specific metrics.
//!
//! # Does NOT
//! - Duplicate `lumas-performance`'s metric tracking infrastructure
//! - Own the global metrics registry

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Runtime-safe counter for metrics collection.
#[derive(Debug)]
pub struct RtSafeCounter {
    count: AtomicU64,
}

impl Clone for RtSafeCounter {
    fn clone(&self) -> Self {
        Self {
            count: AtomicU64::new(self.count.load(Ordering::Relaxed)),
        }
    }
}

impl RtSafeCounter {
    /// Create a new counter initialized to 0.
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }

    /// Increment the counter by 1.
    pub fn increment(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current count.
    pub fn get(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Reset the counter to 0.
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }
}

impl Default for RtSafeCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple gauge for tracking current values.
#[derive(Debug)]
pub struct AsyncGauge<T> {
    value: AtomicU64, // stored as bits for type flexibility
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Clone for AsyncGauge<T> {
    fn clone(&self) -> Self {
        Self {
            value: AtomicU64::new(self.value.load(Ordering::Relaxed)),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl AsyncGauge<f64> {
    /// Create a new gauge initialized to 0.0.
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Set the gauge value.
    pub fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Get the current gauge value.
    pub fn get(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }
}

impl Default for AsyncGauge<f64> {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple histogram for tracking duration distributions.
#[derive(Debug)]
pub struct HdrHistogram {
    buckets: [AtomicU64; 64],
    count: AtomicU64,
}

impl Clone for HdrHistogram {
    fn clone(&self) -> Self {
        // SAFETY: [AtomicU64; 64] doesn't implement Clone in std (arrays > 32),
        // so we use zeroed memory. AtomicU64's all-zero bit pattern is valid (value = 0).
        let mut buckets: [AtomicU64; 64] = unsafe { std::mem::zeroed() };
        for (i, bucket) in self.buckets.iter().enumerate() {
            buckets[i] = AtomicU64::new(bucket.load(Ordering::Relaxed));
        }
        Self {
            buckets,
            count: AtomicU64::new(self.count.load(Ordering::Relaxed)),
        }
    }
}

impl HdrHistogram {
    /// Create a new histogram.
    pub fn new() -> Self {
        const INIT: AtomicU64 = AtomicU64::new(0);
        Self {
            buckets: [INIT; 64],
            count: AtomicU64::new(0),
        }
    }

    /// Record a duration in microseconds.
    pub fn record(&self, value_us: u64) {
        let bucket = (value_us as f64).log2().ceil() as usize;
        let bucket = bucket.min(63);
        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the total number of recorded values.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

impl Default for HdrHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// All metrics collected by the Character Engine.
#[derive(Debug, Clone)]
pub struct CharacterMetrics {
    /// Number of behavior selections.
    pub behavior_selections: RtSafeCounter,
    /// Number of behavior interruptions.
    pub behavior_interruptions: RtSafeCounter,
    /// Number of emotion changes.
    pub emotion_changes: RtSafeCounter,
    /// Tick duration histogram (microseconds).
    pub tick_duration_us: HdrHistogram,
    /// Behavior score evaluation duration histogram (microseconds).
    pub behavior_score_eval_us: HdrHistogram,
    /// Total active time in seconds.
    pub active_time_secs: AsyncGauge<f64>,
    /// Total idle time in seconds.
    pub idle_time_secs: AsyncGauge<f64>,
    /// Number of navigation failures.
    pub navigation_failures: RtSafeCounter,
}

impl CharacterMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            behavior_selections: RtSafeCounter::new(),
            behavior_interruptions: RtSafeCounter::new(),
            emotion_changes: RtSafeCounter::new(),
            tick_duration_us: HdrHistogram::new(),
            behavior_score_eval_us: HdrHistogram::new(),
            active_time_secs: AsyncGauge::new(),
            idle_time_secs: AsyncGauge::new(),
            navigation_failures: RtSafeCounter::new(),
        })
    }

    /// Record the duration of a tick cycle.
    pub fn record_tick(&self, start: Instant) {
        let elapsed = start.elapsed();
        self.tick_duration_us.record(elapsed.as_micros() as u64);
    }

    /// Record a behavior selection.
    pub fn record_behavior_selection(&self) {
        self.behavior_selections.increment();
    }

    /// Record a behavior interruption.
    pub fn record_behavior_interruption(&self) {
        self.behavior_interruptions.increment();
    }

    /// Record an emotion change.
    pub fn record_emotion_change(&self) {
        self.emotion_changes.increment();
    }

    /// Record a navigation failure.
    pub fn record_navigation_failure(&self) {
        self.navigation_failures.increment();
    }
}

impl Default for CharacterMetrics {
    fn default() -> Self {
        Self {
            behavior_selections: RtSafeCounter::new(),
            behavior_interruptions: RtSafeCounter::new(),
            emotion_changes: RtSafeCounter::new(),
            tick_duration_us: HdrHistogram::new(),
            behavior_score_eval_us: HdrHistogram::new(),
            active_time_secs: AsyncGauge::new(),
            idle_time_secs: AsyncGauge::new(),
            navigation_failures: RtSafeCounter::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_safe_counter() {
        let counter = RtSafeCounter::new();
        assert_eq!(counter.get(), 0);
        counter.increment();
        assert_eq!(counter.get(), 1);
        counter.increment();
        assert_eq!(counter.get(), 2);
        counter.reset();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_histogram_records_values() {
        let hist = HdrHistogram::new();
        hist.record(100);
        hist.record(200);
        hist.record(500);
        assert_eq!(hist.count(), 3);
    }

    #[test]
    fn test_gauge_set_and_get() {
        let gauge = AsyncGauge::new();
        gauge.set(42.5);
        assert!((gauge.get() - 42.5).abs() < 0.001);
    }

    #[test]
    fn test_metrics_creation() {
        let metrics = CharacterMetrics::new();
        metrics.record_behavior_selection();
        assert_eq!(metrics.behavior_selections.get(), 1);
    }
}
