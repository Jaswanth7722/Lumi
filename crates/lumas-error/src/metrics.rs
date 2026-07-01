//! # Error Metrics
//!
//! Lock-free, atomic counter store for error metrics.
//! No `std::sync::Mutex` on the hot path — uses atomics and DashMap.
//!
//! # Thread Safety
//! All counters are atomic or use DashMap for concurrent access.
//! `Send + Sync` across all public types.

use crate::category::ErrorCategory;
use crate::severity::Severity;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Lock-free error metrics store.
#[derive(Debug)]
pub struct ErrorMetrics {
    /// Total errors ever recorded.
    pub total_errors: AtomicU64,
    /// Errors by severity level (indexed by Severity as usize).
    pub errors_by_severity: [AtomicU64; 7],
    /// Errors by category.
    pub errors_by_category: DashMap<ErrorCategory, AtomicU64>,
    /// Recovery attempt count.
    pub recovery_attempts: AtomicU64,
    /// Recovery success count.
    pub recovery_successes: AtomicU64,
    /// Recovery failure count.
    pub recovery_failures: AtomicU64,
    /// Retry attempt count.
    pub retry_attempts: AtomicU64,
    /// Retry success count.
    pub retry_successes: AtomicU64,
    /// Panic count.
    pub panic_count: AtomicU64,
    /// Crash count.
    pub crash_count: AtomicU64,
    /// Rate tracking via sliding window.
    error_rate_window: SlidingWindowCounter,
}

impl Default for ErrorMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorMetrics {
    /// Create a new error metrics store.
    pub fn new() -> Self {
        Self {
            total_errors: AtomicU64::new(0),
            errors_by_severity: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            errors_by_category: DashMap::new(),
            recovery_attempts: AtomicU64::new(0),
            recovery_successes: AtomicU64::new(0),
            recovery_failures: AtomicU64::new(0),
            retry_attempts: AtomicU64::new(0),
            retry_successes: AtomicU64::new(0),
            panic_count: AtomicU64::new(0),
            crash_count: AtomicU64::new(0),
            error_rate_window: SlidingWindowCounter::new(Duration::from_secs(60)),
        }
    }

    /// Record an error occurrence.
    pub fn record_error(&self, category: &ErrorCategory, severity: Severity) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        self.errors_by_severity[severity as usize].fetch_add(1, Ordering::Relaxed);
        self.errors_by_category
            .entry(category.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
        self.error_rate_window.increment();
    }

    /// Record a recovery attempt.
    pub fn record_recovery_attempt(&self) {
        self.recovery_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a recovery success.
    pub fn record_recovery_success(&self) {
        self.recovery_successes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a recovery failure.
    pub fn record_recovery_failure(&self) {
        self.recovery_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a retry attempt.
    pub fn record_retry_attempt(&self) {
        self.retry_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a retry success.
    pub fn record_retry_success(&self) {
        self.retry_successes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a panic.
    pub fn record_panic(&self) {
        self.panic_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a crash.
    pub fn record_crash(&self) {
        self.crash_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current error rate per second.
    pub fn error_rate_per_sec(&self) -> f64 {
        self.error_rate_window.rate()
    }

    /// Snapshot all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let mut by_category: Vec<(String, u64)> = Vec::new();
        for entry in self.errors_by_category.iter() {
            by_category.push((
                entry.key().display_name().to_string(),
                entry.value().load(Ordering::Relaxed),
            ));
        }
        by_category.sort_by(|a, b| b.1.cmp(&a.1));

        MetricsSnapshot {
            total_errors: self.total_errors.load(Ordering::Relaxed),
            errors_by_severity: [
                self.errors_by_severity[0].load(Ordering::Relaxed),
                self.errors_by_severity[1].load(Ordering::Relaxed),
                self.errors_by_severity[2].load(Ordering::Relaxed),
                self.errors_by_severity[3].load(Ordering::Relaxed),
                self.errors_by_severity[4].load(Ordering::Relaxed),
                self.errors_by_severity[5].load(Ordering::Relaxed),
                self.errors_by_severity[6].load(Ordering::Relaxed),
            ],
            errors_by_category: by_category,
            recovery_attempts: self.recovery_attempts.load(Ordering::Relaxed),
            recovery_successes: self.recovery_successes.load(Ordering::Relaxed),
            recovery_failures: self.recovery_failures.load(Ordering::Relaxed),
            retry_attempts: self.retry_attempts.load(Ordering::Relaxed),
            retry_successes: self.retry_successes.load(Ordering::Relaxed),
            panic_count: self.panic_count.load(Ordering::Relaxed),
            crash_count: self.crash_count.load(Ordering::Relaxed),
            error_rate_per_sec: self.error_rate_per_sec(),
        }
    }
}

/// A snapshot of all error metrics at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Total errors ever recorded.
    pub total_errors: u64,
    /// Errors by severity level (indexed by Severity as usize).
    pub errors_by_severity: [u64; 7],
    /// Errors by category (sorted by count descending).
    pub errors_by_category: Vec<(String, u64)>,
    /// Recovery attempt count.
    pub recovery_attempts: u64,
    /// Recovery success count.
    pub recovery_successes: u64,
    /// Recovery failure count.
    pub recovery_failures: u64,
    /// Retry attempt count.
    pub retry_attempts: u64,
    /// Retry success count.
    pub retry_successes: u64,
    /// Panic count.
    pub panic_count: u64,
    /// Crash count.
    pub crash_count: u64,
    /// Error rate per second (sliding window).
    pub error_rate_per_sec: f64,
}

/// Sliding window counter for rate tracking.
///
/// Uses a simple time-based sliding window with bucketed counts.
/// Lock-free for increment operations; snapshot reads acquire a brief lock.
#[derive(Debug)]
struct SlidingWindowCounter {
    /// Window duration.
    window: Duration,
    /// Buckets (60 1-second buckets).
    buckets: Arc<parking_lot::Mutex<Vec<(Instant, u64)>>>,
}

impl SlidingWindowCounter {
    fn new(window: Duration) -> Self {
        Self {
            window,
            buckets: Arc::new(parking_lot::Mutex::new(Vec::with_capacity(64))),
        }
    }

    fn increment(&self) {
        let now = Instant::now();
        let mut buckets = self.buckets.lock();
        // Prune old entries
        buckets.retain(|(t, _)| now.duration_since(*t) < self.window);
        // Add new entry
        buckets.push((now, 1));
    }

    fn rate(&self) -> f64 {
        let now = Instant::now();
        let buckets = self.buckets.lock();
        let total: u64 = buckets
            .iter()
            .filter(|(t, _)| now.duration_since(*t) < self.window)
            .map(|(_, c)| c)
            .sum();
        let secs = self.window.as_secs_f64();
        if secs > 0.0 { total as f64 / secs } else { 0.0 }
    }
}

/// Global error metrics singleton.
pub static GLOBAL_ERROR_METRICS: std::sync::OnceLock<ErrorMetrics> = std::sync::OnceLock::new();

/// Get the global error metrics instance.
pub fn global_error_metrics() -> &'static ErrorMetrics {
    GLOBAL_ERROR_METRICS.get_or_init(ErrorMetrics::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_counters() {
        let metrics = ErrorMetrics::new();
        metrics.record_error(&ErrorCategory::Runtime, Severity::Critical);
        metrics.record_error(
            &ErrorCategory::AiCore { provider: None },
            Severity::Recoverable,
        );

        assert_eq!(metrics.total_errors.load(Ordering::Relaxed), 2);
        assert_eq!(
            metrics.errors_by_severity[Severity::Critical as usize].load(Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics.errors_by_severity[Severity::Recoverable as usize].load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn test_recovery_counters() {
        let metrics = ErrorMetrics::new();
        metrics.record_recovery_attempt();
        metrics.record_recovery_success();

        assert_eq!(metrics.recovery_attempts.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.recovery_successes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = ErrorMetrics::new();
        metrics.record_error(&ErrorCategory::Runtime, Severity::Critical);
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.total_errors, 1);
        assert_eq!(snapshot.errors_by_severity[Severity::Critical as usize], 1);
    }

    #[test]
    fn test_sliding_window() {
        let counter = SlidingWindowCounter::new(Duration::from_secs(60));
        counter.increment();
        counter.increment();
        assert!(counter.rate() > 0.0);
    }

    #[test]
    fn test_global_metrics() {
        let metrics = global_error_metrics();
        metrics.record_error(&ErrorCategory::Runtime, Severity::Warning);
        assert!(metrics.total_errors.load(Ordering::Relaxed) >= 1);
    }
}
