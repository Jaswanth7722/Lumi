//! # Desktop Metrics
//!
//! Thread-safe atomic counters for Desktop Engine observability.
//!
//! # Thread Safety
//!
//! All fields use atomic operations for lock-free concurrent access.

use std::sync::atomic::{AtomicU64, Ordering};

/// Desktop engine metrics with atomic counters.
pub struct DesktopMetrics {
    /// Number of windows created.
    pub windows_created: AtomicU64,
    /// Number of windows destroyed.
    pub windows_destroyed: AtomicU64,
    /// Number of overlays created.
    pub overlays_created: AtomicU64,
    /// Number of overlays destroyed.
    pub overlays_destroyed: AtomicU64,
    /// Number of hit tests performed.
    pub hit_tests: AtomicU64,
    /// Number of alpha mask updates.
    pub mask_updates: AtomicU64,
    /// Number of monitor hot-plug events.
    pub monitor_events: AtomicU64,
    /// Number of cursor position updates.
    pub cursor_updates: AtomicU64,
    /// Number of workspace snapshots taken.
    pub workspace_snapshots: AtomicU64,
    /// Number of input hook events.
    pub input_events: AtomicU64,
}

impl DesktopMetrics {
    /// Create a new desktop metrics instance.
    pub fn new() -> Self {
        Self {
            windows_created: AtomicU64::new(0),
            windows_destroyed: AtomicU64::new(0),
            overlays_created: AtomicU64::new(0),
            overlays_destroyed: AtomicU64::new(0),
            hit_tests: AtomicU64::new(0),
            mask_updates: AtomicU64::new(0),
            monitor_events: AtomicU64::new(0),
            cursor_updates: AtomicU64::new(0),
            workspace_snapshots: AtomicU64::new(0),
            input_events: AtomicU64::new(0),
        }
    }
}

impl Default for DesktopMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of desktop metrics at a point in time.
#[derive(Debug, Clone)]
pub struct DesktopMetricsSnapshot {
    /// Total windows created.
    pub windows_created: u64,
    /// Total windows destroyed.
    pub windows_destroyed: u64,
    /// Total overlays created.
    pub overlays_created: u64,
    /// Total hit tests performed.
    pub hit_tests: u64,
    /// Total alpha mask updates.
    pub mask_updates: u64,
    /// Total monitor events.
    pub monitor_events: u64,
    /// Total workspace snapshots.
    pub workspace_snapshots: u64,
}

impl DesktopMetrics {
    /// Take a snapshot of all current metric values.
    pub fn snapshot(&self) -> DesktopMetricsSnapshot {
        DesktopMetricsSnapshot {
            windows_created: self.windows_created.load(Ordering::Relaxed),
            windows_destroyed: self.windows_destroyed.load(Ordering::Relaxed),
            overlays_created: self.overlays_created.load(Ordering::Relaxed),
            hit_tests: self.hit_tests.load(Ordering::Relaxed),
            mask_updates: self.mask_updates.load(Ordering::Relaxed),
            monitor_events: self.monitor_events.load(Ordering::Relaxed),
            workspace_snapshots: self.workspace_snapshots.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_defaults() {
        let m = DesktopMetrics::new();
        assert_eq!(m.windows_created.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let m = DesktopMetrics::new();
        m.windows_created.fetch_add(5, Ordering::Relaxed);
        m.hit_tests.fetch_add(100, Ordering::Relaxed);
        let snap = m.snapshot();
        assert_eq!(snap.windows_created, 5);
        assert_eq!(snap.hit_tests, 100);
    }
}
