//! # Desktop Event Types
//!
//! Desktop-specific event types that implement `lumas_runtime::event::Event`.
//!
//! These events are published on the `EventBus` and can be subscribed to
//! by any subsystem. All events carry timestamps for correlation.

use crate::geometry::{LogicalPoint, PhysicalRect, ScaleFactor};
use crate::monitor::{MonitorId, MonitorInfo};
use crate::observer::ObservedWindow;
use crate::overlay::OverlayId;
use crate::workspace::WorkspaceSnapshot;
use chrono::{DateTime, Utc};
use lumas_runtime::event::{Event, EventPriority};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Emitted when the Desktop Engine has been initialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopInitialized {
    /// Platform identifier (e.g., "windows", "macos", "x11", "wayland").
    pub platform: String,
    /// Number of connected monitors.
    pub monitor_count: u32,
    /// Primary monitor info.
    pub primary_monitor: MonitorInfo,
    /// When initialization completed.
    pub initialized_at: DateTime<Utc>,
}

impl Event for DesktopInitialized {
    fn event_type() -> &'static str {
        "DesktopInitialized"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

/// Emitted when a new monitor is connected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorAdded {
    /// The new monitor info.
    pub monitor: MonitorInfo,
    /// When it was connected.
    pub added_at: DateTime<Utc>,
}

impl Event for MonitorAdded {
    fn event_type() -> &'static str {
        "MonitorAdded"
    }
}

/// Emitted when a monitor is disconnected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRemoved {
    /// The monitor ID.
    pub id: MonitorId,
    /// When it was removed.
    pub removed_at: DateTime<Utc>,
}

impl Event for MonitorRemoved {
    fn event_type() -> &'static str {
        "MonitorRemoved"
    }
}

/// Emitted when a monitor's scale factor changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorScaleChanged {
    /// The monitor ID.
    pub id: MonitorId,
    /// The old scale factor.
    pub old_scale: ScaleFactor,
    /// The new scale factor.
    pub new_scale: ScaleFactor,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for MonitorScaleChanged {
    fn event_type() -> &'static str {
        "MonitorScaleChanged"
    }
}

/// Emitted when a monitor's resolution changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorResolutionChanged {
    /// The monitor ID.
    pub id: MonitorId,
    /// The new physical rect.
    pub new_rect: PhysicalRect,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for MonitorResolutionChanged {
    fn event_type() -> &'static str {
        "MonitorResolutionChanged"
    }
}

/// Emitted when the active/focused window changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWindowChanged {
    /// The previously focused window.
    pub previous: Option<ObservedWindow>,
    /// The currently focused window.
    pub current: Option<ObservedWindow>,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for ActiveWindowChanged {
    fn event_type() -> &'static str {
        "ActiveWindowChanged"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when a foreground application enters/exits fullscreen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullscreenModeChanged {
    /// The fullscreen application.
    pub application: Option<String>,
    /// Whether the application is now in fullscreen.
    pub active: bool,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for FullscreenModeChanged {
    fn event_type() -> &'static str {
        "FullscreenModeChanged"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

/// Emitted when the workspace snapshot is updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshotUpdated {
    /// The new snapshot.
    pub snapshot: Arc<WorkspaceSnapshot>,
    /// Fields that changed since the last snapshot.
    pub changed_fields: Vec<String>,
    /// When the update occurred.
    pub updated_at: DateTime<Utc>,
}

impl Event for WorkspaceSnapshotUpdated {
    fn event_type() -> &'static str {
        "WorkspaceSnapshotUpdated"
    }
}

/// Emitted when an overlay position changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayPositionChanged {
    /// The overlay ID.
    pub id: OverlayId,
    /// The old position.
    pub old_position: LogicalPoint,
    /// The new position.
    pub new_position: LogicalPoint,
    /// When the change occurred.
    pub changed_at: DateTime<Utc>,
}

impl Event for OverlayPositionChanged {
    fn event_type() -> &'static str {
        "OverlayPositionChanged"
    }
}

/// Emitted when the cursor position changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPositionUpdated {
    /// Current cursor position.
    pub position: LogicalPoint,
    /// When the update occurred.
    pub updated_at: DateTime<Utc>,
}

impl Event for CursorPositionUpdated {
    fn event_type() -> &'static str {
        "CursorPositionUpdated"
    }
    fn priority() -> EventPriority {
        EventPriority::Low
    }
}

/// Emitted when accessibility permission is required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityPermissionRequired {
    /// The platform identifier.
    pub platform: String,
    /// The feature requiring permission.
    pub feature: String,
    /// When the request was made.
    pub requested_at: DateTime<Utc>,
}

impl Event for AccessibilityPermissionRequired {
    fn event_type() -> &'static str {
        "AccessibilityPermissionRequired"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_names() {
        assert_eq!(DesktopInitialized::event_type(), "DesktopInitialized");
        assert_eq!(MonitorAdded::event_type(), "MonitorAdded");
        assert_eq!(ActiveWindowChanged::event_type(), "ActiveWindowChanged");
        assert_eq!(CursorPositionUpdated::event_type(), "CursorPositionUpdated");
    }
}
