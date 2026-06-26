//! # Timer and Span Measurement
//!
//! Provides `Timer` with RAII guard-based duration recording and an async
//! wrapper for measuring future execution time.
//!
//! # Thread Safety
//! `Timer` is `Send + Sync`. `TimerGuard` is not `Send` (must be dropped on
//! the same thread it was created).

use crate::histogram::HdrHistogram;
use crate::metric::{MetricKind, MetricName, MetricSnapshot, MetricUnit, Tag};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// A timer that records durations to an HDR histogram.
#[derive(Clone)]
pub struct Timer {
    name: MetricName,
    histogram: Arc<HdrHistogram>,
}

impl Timer {
    /// Create a new timer backed by the given histogram.
    pub fn new(name: MetricName) -> Self {
        Self {
            name,
            histogram: Arc::new(HdrHistogram::new()),
        }
    }

    /// Create a timer with a pre-configured histogram.
    pub fn with_histogram(name: MetricName, histogram: Arc<HdrHistogram>) -> Self {
        Self { name, histogram }
    }

    /// Start a timer, returning a guard that records the elapsed time on drop.
    pub fn start(&self) -> TimerGuard<'_> {
        TimerGuard {
            timer: self,
            start: Instant::now(),
            cancelled: false,
        }
    }

    /// Record a pre-measured duration.
    pub fn record(&self, duration: Duration) -> Result<(), crate::PerformanceError> {
        let micros = duration.as_micros() as u64;
        self.histogram.record(micros)
    }

    /// Record a duration in microseconds.
    pub fn record_micros(&self, micros: u64) -> Result<(), crate::PerformanceError> {
        self.histogram.record(micros)
    }

    /// Wrap a future and record its execution time.
    pub async fn time_async<F, T>(&self, fut: F) -> T
    where
        F: Future<Output = T>,
    {
        let start = Instant::now();
        let result = fut.await;
        let _ = self.record(start.elapsed());
        result
    }

    /// Get the underlying histogram for percentile queries.
    pub fn histogram(&self) -> &Arc<HdrHistogram> {
        &self.histogram
    }

    /// Get the timer name.
    pub fn name(&self) -> &MetricName {
        &self.name
    }
}

/// RAII guard that records elapsed time when dropped.
///
/// # Performance
/// Drop is O(1) — reads `Instant::now()`, computes delta, records to histogram.
#[must_use = "TimerGuard records on drop; dropping immediately measures 0 duration"]
pub struct TimerGuard<'a> {
    timer: &'a Timer,
    start: Instant,
    cancelled: bool,
}

impl<'a> TimerGuard<'a> {
    /// Cancel this measurement (prevents recording on drop).
    /// Useful when the operation failed and timing is not meaningful.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

impl<'a> Drop for TimerGuard<'a> {
    fn drop(&mut self) {
        if !self.cancelled {
            let _ = self.timer.record(self.start.elapsed());
        }
    }
}

/// Macro: time the execution of a block and record its duration.
///
/// # Example
/// ```ignore
/// let result = time!(metrics.ai.first_token, {
///     provider.complete(request).await
/// });
/// ```
#[macro_export]
macro_rules! time {
    ($timer:expr, $block:block) => {{
        let __guard = $timer.start();
        let __result = $block;
        drop(__guard);
        __result
    }};
}

impl std::fmt::Debug for Timer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Timer({})", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_timer_guard_records_duration() {
        let timer = Timer::new(MetricName::from_str("lumi.test.op.microseconds"));
        {
            let _guard = timer.start();
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let snap = timer.histogram().snapshot();
        assert!(snap.count > 0);
    }

    #[test]
    fn test_timer_guard_cancel() {
        let timer = Timer::new(MetricName::from_str("lumi.test.op.microseconds"));
        {
            let mut guard = timer.start();
            guard.cancel();
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let snap = timer.histogram().snapshot();
        assert_eq!(snap.count, 0);
    }

    #[test]
    fn test_timer_record_direct() {
        let timer = Timer::new(MetricName::from_str("lumi.test.op.microseconds"));
        timer.record(Duration::from_micros(100)).unwrap();
        let snap = timer.histogram().snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.min, 100);
    }

    #[tokio::test]
    async fn test_timer_async() {
        let timer = Timer::new(MetricName::from_str("lumi.test.op.microseconds"));
        let result = timer
            .time_async(async {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                42
            })
            .await;
        assert_eq!(result, 42);
        let snap = timer.histogram().snapshot();
        assert!(snap.count > 0);
    }
}
