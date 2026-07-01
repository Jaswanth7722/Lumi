//! # Test Utilities
//!
//! Test helpers for lumas-performance:
//! - `FakeSystemSampler` — returns configurable deterministic samples
//! - `MockExporter` — captures export calls for assertion
//! - `inject_metric_value!` — bypass normal collection for threshold testing
//!
//! # Thread Safety
//! All test utilities are `Send + Sync`.

use crate::error::PerformanceResult;
use crate::export::MetricExporter;
use crate::manager::PerformanceSnapshot;
use async_trait::async_trait;

/// Mock exporter that captures export calls in memory.
#[derive(Debug)]
pub struct MockExporter {
    /// Name of this exporter.
    name: &'static str,
    /// Number of exports called.
    pub export_count: std::sync::atomic::AtomicU64,
    /// Last exported snapshot.
    pub last_snapshot: parking_lot::Mutex<Option<PerformanceSnapshot>>,
}

impl MockExporter {
    /// Create a new mock exporter.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            export_count: std::sync::atomic::AtomicU64::new(0),
            last_snapshot: parking_lot::Mutex::new(None),
        }
    }
}

#[async_trait]
impl MetricExporter for MockExporter {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn export(&self, snapshot: &PerformanceSnapshot) -> PerformanceResult<()> {
        self.export_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.last_snapshot.lock() = Some(snapshot.clone());
        Ok(())
    }
}
