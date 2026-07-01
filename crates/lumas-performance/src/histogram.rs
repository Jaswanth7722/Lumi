//! # HDR Histogram
//!
//! High Dynamic Range Histogram with O(1) recording and percentile queries.
//! Uses the `hdrhistogram` crate internally with lock-free thread-local recording
//! for real-time safety.
//!
//! # Thread Safety
//! `HdrHistogram` uses internal synchronization. `RtSafeHistogram` uses a
//! lock-free ring buffer for thread-safe recording without allocation.

use crate::metric::HistogramBucketSnapshot;
use hdrhistogram::Histogram;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A fixed-size ring buffer slot for lock-free histogram recording.
const RING_BUFFER_SIZE: usize = 1024;

/// Wraps the `hdrhistogram` crate with the Lumas metric trait.
///
/// Suitable for background subsystems. For real-time paths, use `RtSafeHistogram`.
#[derive(Clone)]
pub struct HdrHistogram {
    inner: Arc<std::sync::Mutex<Histogram<u64>>>,
}

impl fmt::Debug for HdrHistogram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HdrHistogram").finish()
    }
}

impl HdrHistogram {
    /// Create a new HDR histogram with default range (1µs to 60s).
    pub fn new() -> Self {
        let hist = Histogram::new_with_bounds(1, 60_000_000, 3).unwrap();
        Self {
            inner: Arc::new(std::sync::Mutex::new(hist)),
        }
    }

    /// Create a histogram with custom value range and precision.
    pub fn with_bounds(
        low: u64,
        high: u64,
        precision: u8,
    ) -> Result<Self, crate::PerformanceError> {
        let hist = Histogram::new_with_bounds(low, high, precision)
            .map_err(|e| crate::PerformanceError::Internal(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(std::sync::Mutex::new(hist)),
        })
    }

    /// Record a value in microseconds.
    /// Returns an error if the value exceeds the histogram's configured maximum.
    pub fn record(&self, value: u64) -> Result<(), crate::PerformanceError> {
        let mut hist = self.inner.lock().unwrap();
        hist.record(value)
            .map_err(|_| crate::PerformanceError::HistogramRangeMismatch {
                metric: String::new(),
                value,
                max: hist.high(),
            })
    }

    /// Record a value `n` times.
    /// Returns an error if the value exceeds the histogram's configured maximum.
    pub fn record_n(&self, value: u64, count: u64) -> Result<(), crate::PerformanceError> {
        let mut hist = self.inner.lock().unwrap();
        hist.record_n(value, count)
            .map_err(|_| crate::PerformanceError::HistogramRangeMismatch {
                metric: String::new(),
                value,
                max: hist.high(),
            })
    }

    /// Get the value at the given percentile (0.0–100.0).
    pub fn percentile(&self, p: f64) -> Option<u64> {
        let hist = self.inner.lock().unwrap();
        if hist.len() == 0 {
            return None;
        }
        Some(hist.value_at_percentile(p))
    }

    /// Get the mean of recorded values.
    pub fn mean(&self) -> f64 {
        let hist = self.inner.lock().unwrap();
        hist.mean()
    }

    /// Get the standard deviation.
    pub fn stddev(&self) -> f64 {
        let hist = self.inner.lock().unwrap();
        hist.stdev()
    }

    /// Get the maximum recorded value.
    pub fn max(&self) -> u64 {
        let hist = self.inner.lock().unwrap();
        hist.max()
    }

    /// Get the minimum recorded value.
    pub fn min(&self) -> u64 {
        let hist = self.inner.lock().unwrap();
        hist.min()
    }

    /// Get the number of recorded values.
    pub fn count(&self) -> u64 {
        let hist = self.inner.lock().unwrap();
        hist.len()
    }

    /// Snapshot histogram percentiles.
    pub fn snapshot(&self) -> HistogramBucketSnapshot {
        let hist = self.inner.lock().unwrap();
        if hist.len() == 0 {
            return HistogramBucketSnapshot::default();
        }
        HistogramBucketSnapshot {
            count: hist.len(),
            min: hist.min(),
            max: hist.max(),
            mean: hist.mean(),
            stddev: hist.stdev(),
            p50: hist.value_at_percentile(50.0),
            p90: hist.value_at_percentile(90.0),
            p95: hist.value_at_percentile(95.0),
            p99: hist.value_at_percentile(99.0),
            p999: hist.value_at_percentile(99.9),
        }
    }

    /// Reset the histogram.
    pub fn reset(&self) {
        let mut hist = self.inner.lock().unwrap();
        hist.reset();
    }

    /// Merge histograms from multiple recorders into this one.
    pub fn merge(&self, other: &HdrHistogram) {
        let mut self_hist = self.inner.lock().unwrap();
        let other_hist = other.inner.lock().unwrap();
        let _ = self_hist.add(other_hist.clone());
    }
}

/// Real-time-safe histogram using a pre-allocated ring buffer.
///
/// # Performance
/// - Recording: single `AtomicU64` CAS into a fixed-size ring buffer
/// - No allocation, no locks on the recording path
/// - If buffer is full, oldest sample is silently dropped
///
/// # Thread Safety
/// Lock-free via atomic operations. Safe to share across threads.
/// The ring buffer uses a monotonic write index — concurrent `record()` calls
/// may overwrite each other's slots (acceptable for statistical sampling).
#[derive(Debug)]
pub struct RtSafeHistogram {
    /// Ring buffer of recorded values.
    buffer: Box<[AtomicU64; RING_BUFFER_SIZE]>,
    /// Write index (atomically incremented).
    write_idx: AtomicU64,
}

impl RtSafeHistogram {
    /// Create a new real-time-safe histogram with 1024 slots.
    pub fn new() -> Self {
        // SAFETY: AtomicU64 can be safely zero-initialized
        let mut buf = Box::new(unsafe { std::mem::zeroed::<[AtomicU64; RING_BUFFER_SIZE]>() });
        for slot in buf.iter_mut() {
            *slot = AtomicU64::new(0);
        }
        Self {
            buffer: buf,
            write_idx: AtomicU64::new(0),
        }
    }

    /// Record a value. If the buffer is full, silently drops the oldest sample.
    #[inline(always)]
    pub fn record(&self, value: u64) {
        let idx = self.write_idx.fetch_add(1, Ordering::Relaxed) as usize % RING_BUFFER_SIZE;
        self.buffer[idx].store(value, Ordering::Relaxed);
    }

    /// Drain the ring buffer into an HDR histogram and return it.
    /// The ring buffer is not cleared — the caller should call `reset()` if needed.
    pub fn drain_to_histogram(&self) -> HdrHistogram {
        let hist = HdrHistogram::new();
        let count = self.write_idx.load(Ordering::Relaxed);
        let start = if count > RING_BUFFER_SIZE as u64 {
            count as usize - RING_BUFFER_SIZE
        } else {
            0
        };
        for i in start..count as usize {
            let val = self.buffer[i % RING_BUFFER_SIZE].load(Ordering::Relaxed);
            if val > 0 {
                let _ = hist.record(val);
            }
        }
        hist
    }

    /// Reset the buffer.
    pub fn reset(&self) {
        self.write_idx.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hdr_histogram_record() {
        let hist = HdrHistogram::new();
        hist.record(100).unwrap();
        hist.record(200).unwrap();
        hist.record(300).unwrap();
        assert_eq!(hist.count(), 3);
        assert!(hist.mean() > 0.0);
    }

    #[test]
    fn test_hdr_histogram_percentiles() {
        let hist = HdrHistogram::new();
        for v in [100, 200, 300, 400, 500] {
            hist.record(v).unwrap();
        }
        let p50 = hist.percentile(50.0).unwrap();
        assert!(p50 >= 100 && p50 <= 500);
        let p100 = hist.percentile(100.0).unwrap();
        assert_eq!(p100, 500);
    }

    #[test]
    fn test_rt_safe_histogram_record() {
        let hist = RtSafeHistogram::new();
        hist.record(42);
        let hdr = hist.drain_to_histogram();
        assert!(hdr.count() > 0);
    }

    #[test]
    fn test_histogram_snapshot() {
        let hist = HdrHistogram::new();
        hist.record(1000).unwrap();
        let snap = hist.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.min, 1000);
        assert_eq!(snap.max, 1000);
    }

    #[test]
    fn test_rt_safe_ring_buffer_overflow() {
        let hist = RtSafeHistogram::new();
        for i in 0..1500 {
            hist.record(i);
        }
        let hdr = hist.drain_to_histogram();
        assert!(hdr.count() > 0);
        // Should not have panicked
    }
}
