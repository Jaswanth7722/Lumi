// ── Wire Metrics ───────────────────────────────────────────────────────────────
// Wire-level metrics counters for encoding/decoding performance monitoring.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Wire-level metrics.
#[derive(Debug, Clone)]
pub struct WireMetrics {
    inner: Arc<WireMetricsInner>,
}

#[derive(Debug, Default)]
struct WireMetricsInner {
    frames_decoded: AtomicU64,
    frames_encoded: AtomicU64,
    checksum_passes: AtomicU64,
    checksum_failures: AtomicU64,
    truncated_detected: AtomicU64,
    desync_events: AtomicU64,
    oversized_detected: AtomicU64,
    decompression_bombs: AtomicU64,
    decryption_failures: AtomicU64,
    fragmentation_events: AtomicU64,
    reassembly_events: AtomicU64,
    reassembly_timeouts: AtomicU64,
}

/// Snapshot of wire metrics at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricsSnapshot {
    pub frames_decoded: u64,
    pub frames_encoded: u64,
    pub checksum_passes: u64,
    pub checksum_failures: u64,
    pub truncated_detected: u64,
    pub desync_events: u64,
    pub oversized_detected: u64,
    pub decompression_bombs: u64,
    pub decryption_failures: u64,
    pub fragmentation_events: u64,
    pub reassembly_events: u64,
    pub reassembly_timeouts: u64,
}

impl WireMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(WireMetricsInner::default()),
        }
    }

    /// Increment the frames decoded counter.
    pub fn increment_frames_decoded(&self) {
        self.inner.frames_decoded.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the frames encoded counter.
    pub fn increment_frames_encoded(&self) {
        self.inner.frames_encoded.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the checksum passes counter.
    pub fn increment_checksum_passes(&self) {
        self.inner.checksum_passes.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the checksum failures counter.
    pub fn increment_checksum_failures(&self) {
        self.inner.checksum_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the truncated detected counter.
    pub fn increment_truncated_detected(&self) {
        self.inner.truncated_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the desync events counter.
    pub fn increment_desync_events(&self) {
        self.inner.desync_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the oversized detected counter.
    pub fn increment_oversized_detected(&self) {
        self.inner.oversized_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the decompression bombs counter.
    pub fn increment_decompression_bombs(&self) {
        self.inner.decompression_bombs.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the decryption failures counter.
    pub fn increment_decryption_failures(&self) {
        self.inner.decryption_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the fragmentation events counter.
    pub fn increment_fragmentation_events(&self) {
        self.inner.fragmentation_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the reassembly events counter.
    pub fn increment_reassembly_events(&self) {
        self.inner.reassembly_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the reassembly timeouts counter.
    pub fn increment_reassembly_timeouts(&self) {
        self.inner.reassembly_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot all current counters.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            frames_decoded: self.inner.frames_decoded.load(Ordering::Relaxed),
            frames_encoded: self.inner.frames_encoded.load(Ordering::Relaxed),
            checksum_passes: self.inner.checksum_passes.load(Ordering::Relaxed),
            checksum_failures: self.inner.checksum_failures.load(Ordering::Relaxed),
            truncated_detected: self.inner.truncated_detected.load(Ordering::Relaxed),
            desync_events: self.inner.desync_events.load(Ordering::Relaxed),
            oversized_detected: self.inner.oversized_detected.load(Ordering::Relaxed),
            decompression_bombs: self.inner.decompression_bombs.load(Ordering::Relaxed),
            decryption_failures: self.inner.decryption_failures.load(Ordering::Relaxed),
            fragmentation_events: self.inner.fragmentation_events.load(Ordering::Relaxed),
            reassembly_events: self.inner.reassembly_events.load(Ordering::Relaxed),
            reassembly_timeouts: self.inner.reassembly_timeouts.load(Ordering::Relaxed),
        }
    }
}
