// ── Lumi Wire Protocol Diagnostics ──────────────────────────────────────────────
// Wire Diagnostics: Traces wire-level frame events for debugging.
// WARNING: Heavy logging; never enable in production without rate limiting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::wire::error::WireError;
use crate::wire::protocol::*;

/// Diagnostics collector for wire-level events.
#[derive(Debug, Clone, Default)]
pub struct WireDiagnostics {
    inner: Arc<WireDiagnosticsInner>,
}

#[derive(Debug, Default)]
struct WireDiagnosticsInner {
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

impl WireDiagnostics {
    /// Create a new diagnostics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decoded frame.
    pub fn record_frame_decoded(&self) {
        self.inner.frames_decoded.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an encoded frame.
    pub fn record_frame_encoded(&self) {
        self.inner.frames_encoded.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a checksum event.
    pub fn record_checksum(&self, passed: bool) {
        if passed {
            self.inner.checksum_passes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.inner.checksum_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a truncation detection.
    pub fn record_truncated(&self) {
        self.inner.truncated_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a stream desync event.
    pub fn record_desync(&self) {
        self.inner.desync_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an oversized frame detection.
    pub fn record_oversized(&self) {
        self.inner.oversized_detected.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a decompression bomb detection.
    pub fn record_decompression_bomb(&self) {
        self.inner.decompression_bombs.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a decryption failure.
    pub fn record_decryption_failure(&self) {
        self.inner.decryption_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a fragmentation event.
    pub fn record_fragmentation(&self) {
        self.inner.fragmentation_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a reassembly event.
    pub fn record_reassembly(&self) {
        self.inner.reassembly_events.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a reassembly timeout.
    pub fn record_reassembly_timeout(&self) {
        self.inner.reassembly_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot all current counters.
    pub fn snapshot(&self) -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
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

/// A point-in-time snapshot of wire diagnostics counters.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticsSnapshot {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_default() {
        let d = WireDiagnostics::new();
        let snap = d.snapshot();
        assert_eq!(snap.frames_decoded, 0);
        assert_eq!(snap.checksum_failures, 0);
    }

    #[test]
    fn test_diagnostics_increment() {
        let d = WireDiagnostics::new();
        d.record_frame_decoded();
        d.record_frame_decoded();
        d.record_checksum(false);
        assert_eq!(d.snapshot().frames_decoded, 2);
        assert_eq!(d.snapshot().checksum_failures, 1);
    }

    #[test]
    fn test_diagnostics_clone_is_independent() {
        let d1 = WireDiagnostics::new();
        let d2 = d1.clone();
        d1.record_frame_decoded();
        assert_eq!(d2.snapshot().frames_decoded, 0);
        assert_eq!(d1.snapshot().frames_decoded, 1);
    }
}
