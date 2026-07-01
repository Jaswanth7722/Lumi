//! # Monitor Manager
//!
//! Manages display topology — monitor enumeration, hot-plug detection,
//! scale factor tracking, and coordinate-space queries.
//!
//! # Thread Safety
//!
//! `MonitorManager` is `Send + Sync` via `ArcSwap` for lock-free reads.

use crate::error::DesktopError;
use crate::geometry::{
    LogicalPoint, LogicalRect, PhysicalPoint, PhysicalRect, Point, Rect, ScaleFactor, Size,
};
use crate::metrics::DesktopMetrics;
use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use lumas_runtime::event::EventBus;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// MonitorId
// ---------------------------------------------------------------------------

/// A unique identifier for a display monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(Uuid);

impl MonitorId {
    /// Create a new unique monitor ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for MonitorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

impl Default for MonitorId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MonitorInfo
// ---------------------------------------------------------------------------

/// Full metadata about a connected display monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Unique monitor identifier.
    pub id: MonitorId,
    /// Human-readable monitor name (e.g., "DELL U2723QE").
    pub name: String,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
    /// Full monitor bounds in physical pixels.
    pub physical_rect: PhysicalRect,
    /// Work area (excluding taskbar/dock) in logical pixels.
    pub work_area: LogicalRect,
    /// Scale factor for this monitor.
    pub scale_factor: ScaleFactor,
    /// Refresh rate in Hz, if available.
    pub refresh_rate_hz: Option<f64>,
    /// Color depth in bits.
    pub color_depth: u32,
    /// When this monitor was connected.
    pub connected_at: DateTime<Utc>,
}

impl MonitorInfo {
    /// Convert a logical point to physical pixels on this monitor.
    pub fn to_physical(&self, logical: LogicalPoint) -> PhysicalPoint {
        self.scale_factor.point_to_physical(logical)
    }

    /// Convert a physical point to logical pixels on this monitor.
    pub fn to_logical(&self, physical: PhysicalPoint) -> LogicalPoint {
        self.scale_factor.point_to_logical(physical)
    }

    /// Returns `true` if the given logical point is within this monitor's bounds.
    pub fn contains_logical(&self, point: LogicalPoint) -> bool {
        let logical_rect = self.scale_factor.size_to_logical(self.physical_rect.size);
        let rect = Rect::new(
            self.scale_factor.point_to_logical(self.physical_rect.origin),
            logical_rect,
        );
        rect.contains(point)
    }

    /// Returns the logical bounds of this monitor.
    pub fn logical_bounds(&self) -> LogicalRect {
        let origin = self.scale_factor.point_to_logical(self.physical_rect.origin);
        let size = self.scale_factor.size_to_logical(self.physical_rect.size);
        Rect::new(origin, size)
    }
}

// ---------------------------------------------------------------------------
// MonitorEvent
// ---------------------------------------------------------------------------

/// Events related to monitor changes.
#[derive(Debug, Clone)]
pub enum MonitorEvent {
    /// A new monitor was connected.
    Added(MonitorInfo),
    /// A monitor was disconnected.
    Removed(MonitorId),
    /// A monitor's scale factor changed.
    ScaleChanged {
        /// The monitor ID.
        id: MonitorId,
        /// The new scale factor.
        new_scale: ScaleFactor,
    },
    /// A monitor's resolution changed.
    ResolutionChanged {
        /// The monitor ID.
        id: MonitorId,
        /// The new physical rect.
        new_rect: PhysicalRect,
    },
}

// ---------------------------------------------------------------------------
// MonitorManager
// ---------------------------------------------------------------------------

/// Manages display topology and monitor information.
///
/// # Examples
///
/// ```ignore
/// let manager = MonitorManager::new(event_bus, metrics);
/// manager.refresh().await?;
/// let primary = manager.primary();
/// ```
pub struct MonitorManager {
    /// List of all connected monitors (lock-free read via ArcSwap).
    monitors: ArcSwap<Vec<MonitorInfo>>,
    /// ID of the primary monitor.
    primary_id: ArcSwap<Option<MonitorId>>,
    /// Event bus for emitting monitor events.
    event_bus: Arc<EventBus>,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

impl MonitorManager {
    /// Create a new monitor manager.
    pub fn new(event_bus: Arc<EventBus>, metrics: Arc<DesktopMetrics>) -> Self {
        Self {
            monitors: ArcSwap::new(Arc::new(Vec::new())),
            primary_id: ArcSwap::new(Arc::new(None)),
            event_bus,
            metrics,
        }
    }

    /// Enumerate all connected monitors from the OS.
    /// Called at startup and on monitor hot-plug events.
    pub async fn refresh(&self) -> Result<(), DesktopError> {
        // In production, this would use winit's event_loop.available_monitors()
        // or platform-specific enumeration APIs.
        // For now, this is populated by the platform backend.
        Ok(())
    }

    /// Returns all currently connected monitors.
    pub fn all(&self) -> Arc<Vec<MonitorInfo>> {
        self.monitors.load_full()
    }

    /// Returns the primary monitor, if any.
    pub fn primary(&self) -> Option<MonitorInfo> {
        let primary_id = self.primary_id.load_full();
        let monitors = self.monitors.load_full();
        primary_id.and_then(|id| monitors.iter().find(|m| m.id == id).cloned())
    }

    /// Returns the monitor containing the given logical point.
    /// On overlap (rare with mirrored displays), returns the primary monitor.
    pub fn containing(&self, point: LogicalPoint) -> Option<MonitorInfo> {
        let monitors = self.monitors.load_full();
        // First, check explicit containment.
        for monitor in monitors.iter() {
            if monitor.contains_logical(point) {
                return Some(monitor.clone());
            }
        }
        // Fall back to primary monitor.
        self.primary()
    }

    /// Returns the monitor closest to the given point (for off-screen clamping).
    pub fn nearest(&self, point: LogicalPoint) -> Option<MonitorInfo> {
        let monitors = self.monitors.load_full();
        monitors
            .iter()
            .min_by(|a, b| {
                let da = a.logical_bounds().edge_distance(point);
                let db = b.logical_bounds().edge_distance(point);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Returns the combined bounding rect of all monitors in logical pixels.
    pub fn virtual_desktop_bounds(&self) -> LogicalRect {
        let monitors = self.monitors.load_full();
        let mut bounds = monitors
            .first()
            .map(|m| m.logical_bounds())
            .unwrap_or_default();

        for monitor in monitors.iter().skip(1) {
            bounds = bounds.union(&monitor.logical_bounds());
        }
        bounds
    }

    /// Called by the platform backend when a monitor is added or removed.
    pub(crate) fn on_monitor_event(&self, event: MonitorEvent) {
        match event {
            MonitorEvent::Added(info) => {
                let mut monitors = self.monitors.load_full().as_ref().clone();
                monitors.push(info);
                self.monitors.store(Arc::new(monitors));
            }
            MonitorEvent::Removed(id) => {
                let mut monitors = self.monitors.load_full().as_ref().clone();
                monitors.retain(|m| m.id != id);
                self.monitors.store(Arc::new(monitors));
            }
            MonitorEvent::ScaleChanged { id, new_scale } => {
                let mut monitors = self.monitors.load_full().as_ref().clone();
                if let Some(monitor) = monitors.iter_mut().find(|m| m.id == id) {
                    monitor.scale_factor = new_scale;
                }
                self.monitors.store(Arc::new(monitors));
            }
            MonitorEvent::ResolutionChanged { id, new_rect } => {
                let mut monitors = self.monitors.load_full().as_ref().clone();
                if let Some(monitor) = monitors.iter_mut().find(|m| m.id == id) {
                    monitor.physical_rect = new_rect;
                }
                self.monitors.store(Arc::new(monitors));
            }
        }
    }
}

impl std::fmt::Debug for MonitorManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorManager")
            .field("monitor_count", &self.monitors.load_full().len())
            .finish()
    }
}
