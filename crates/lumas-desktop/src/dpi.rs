//! # DPI Context
//!
//! Tracks per-monitor scale factors and provides logical↔physical pixel
//! conversion. Updated on monitor DPI changes (e.g., system display settings).
//!
//! # Thread Safety
//!
//! `DpiContext` is `Send + Sync` via `DashMap`. Scale factor lookup is O(1).

use crate::geometry::{LogicalPoint, LogicalSize, PhysicalPoint, PhysicalSize, Point, ScaleFactor, Size};
use crate::monitor::{MonitorId, MonitorManager};
use dashmap::DashMap;
use lumas_runtime::event::EventBus;
use std::sync::Arc;

/// Tracks per-monitor DPI scale factors for logical↔physical conversion.
pub struct DpiContext {
    /// Per-monitor scale factors.
    scale_factors: DashMap<MonitorId, ScaleFactor>,
    /// Event bus for emitting scale change events.
    event_bus: Arc<EventBus>,
}

impl DpiContext {
    /// Create a new DPI context.
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            scale_factors: DashMap::new(),
            event_bus,
        }
    }

    /// Returns the scale factor for the monitor containing a logical point.
    /// Returns 1.0 as a safe fallback if no monitor is found.
    pub fn scale_at(&self, point: LogicalPoint, monitors: &MonitorManager) -> ScaleFactor {
        if let Some(monitor) = monitors.containing(point) {
            self.scale_factors
                .get(&monitor.id)
                .map(|e| *e.value())
                .unwrap_or(monitor.scale_factor)
        } else {
            ScaleFactor::ONE
        }
    }

    /// Returns the scale factor for a specific monitor.
    pub fn scale_for(&self, id: &MonitorId) -> Option<ScaleFactor> {
        self.scale_factors.get(id).map(|e| *e.value())
    }

    /// Convert a logical size to physical pixels using the scale at a point.
    pub fn to_physical_size(
        &self,
        size: LogicalSize,
        point: LogicalPoint,
        monitors: &MonitorManager,
    ) -> PhysicalSize {
        let scale = self.scale_at(point, monitors);
        scale.size_to_physical(size)
    }

    /// Called when a monitor's DPI changes (e.g., system display settings change).
    pub fn on_scale_changed(&self, id: MonitorId, new_scale: ScaleFactor) {
        self.scale_factors.insert(id, new_scale);
    }
}

impl std::fmt::Debug for DpiContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DpiContext")
            .field("monitor_count", &self.scale_factors.len())
            .finish()
    }
}
