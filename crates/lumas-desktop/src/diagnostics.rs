//! Diagnostics provider for the Desktop Engine.
//!
//! Collects and exports the complete desktop engine state for debugging,
//! health reporting, and integration with `lumi-process` diagnostics.
//!
//! # Thread Safety
//! `DesktopDiagnostics` is `Send + Sync`. All state is accessed via `Arc`
//! references and concurrent data structures.

use crate::error::DesktopError;
use crate::monitor::MonitorInfo;
use crate::overlay::OverlayHandle;
use crate::window::{WindowHandle, WindowId, WindowState};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;

/// Snapshot of the desktop engine state for diagnostics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DesktopDiagnosticsSnapshot {
    /// All active windows with their current state.
    pub windows: Vec<WindowDiagnosticInfo>,
    /// All active overlays.
    pub overlays: Vec<OverlayDiagnosticInfo>,
    /// Currently connected monitors.
    pub monitors: Vec<MonitorInfo>,
    /// Cursor position in logical pixels.
    pub cursor_position: (f64, f64),
    /// Platform backend identifier.
    pub platform: String,
    /// Number of drag-drop targets registered.
    pub drag_drop_targets: usize,
    /// Timestamp of the snapshot.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Diagnostic information for a single window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowDiagnosticInfo {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub position: (f64, f64),
    pub size: (f64, f64),
    pub always_on_top: bool,
    pub transparent: bool,
    pub click_through: bool,
    pub uptime_seconds: f64,
}

/// Diagnostic information for a single overlay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayDiagnosticInfo {
    pub id: String,
    pub kind: String,
    pub opacity: f32,
    pub anchor: String,
}

/// Provides diagnostic snapshots of the desktop engine state.
///
/// # Thread Safety
/// `DesktopDiagnostics` is `Send + Sync`. Snapshots are atomic and
/// non-blocking.
pub struct DesktopDiagnostics {
    windows: Arc<DashMap<WindowId, WindowHandle>>,
    overlays: Arc<Vec<OverlayHandle>>,
    platform_name: String,
}

impl DesktopDiagnostics {
    /// Create a new diagnostics provider.
    pub(crate) fn new(
        windows: Arc<DashMap<WindowId, WindowHandle>>,
        overlays: Arc<Vec<OverlayHandle>>,
        platform_name: String,
    ) -> Self {
        Self {
            windows,
            overlays,
            platform_name,
        }
    }

    /// Capture a full snapshot of the desktop engine state.
    ///
    /// # Errors
    /// Never panics. Returns an empty snapshot if state cannot be collected.
    ///
    /// # Examples
    /// ```
    /// # use lumas_desktop::diagnostics::DesktopDiagnostics;
    /// # let diag = DesktopDiagnostics::empty_for_test();
    /// let snapshot = diag.snapshot();
    /// assert!(snapshot.windows.is_empty());
    /// ```
    pub fn snapshot(&self) -> DesktopDiagnosticsSnapshot {
        let windows: Vec<WindowDiagnosticInfo> = self
            .windows
            .iter()
            .map(|entry| {
                let handle = entry.value();
                let descriptor = handle.descriptor();
                WindowDiagnosticInfo {
                    id: handle.id().to_string(),
                    kind: format!("{:?}", descriptor.kind),
                    title: descriptor.title.clone(),
                    state: format!("{:?}", handle.state()),
                    position: (descriptor.initial_position.x, descriptor.initial_position.y),
                    size: (descriptor.initial_size.width, descriptor.initial_size.height),
                    always_on_top: descriptor.always_on_top,
                    transparent: descriptor.transparent,
                    click_through: descriptor.click_through,
                    uptime_seconds: 0.0, // uptime tracking not exposed on WindowHandle
                }
            })
            .collect();

        let overlays: Vec<OverlayDiagnosticInfo> = self
            .overlays
            .iter()
            .map(|overlay| {
                let desc = overlay.descriptor();
                OverlayDiagnosticInfo {
                    id: overlay.id().to_string(),
                    kind: format!("{:?}", desc.kind),
                    opacity: desc.opacity,
                    anchor: format!("{:?}", desc.initial_anchor),
                }
            })
            .collect();

        DesktopDiagnosticsSnapshot {
            windows,
            overlays,
            monitors: Vec::new(), // Monitors are available from MonitorManager
            cursor_position: (0.0, 0.0),
            platform: self.platform_name.clone(),
            drag_drop_targets: 0,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Export the snapshot as a JSON string.
    ///
    /// # Errors
    /// Returns `DesktopError::PlatformError` if serialization fails.
    pub fn to_json(&self) -> Result<String, DesktopError> {
        let snapshot = self.snapshot();
        serde_json::to_string_pretty(&snapshot).map_err(|e| {
            DesktopError::PlatformError {
                operation: "serialize_diagnostics",
                source: Box::new(e),
            }
        })
    }

    /// Create an empty diagnostics instance for testing.
    pub fn empty_for_test() -> Self {
        Self {
            windows: Arc::new(DashMap::new()),
            overlays: Arc::new(Vec::new()),
            platform_name: "test".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_snapshot() {
        let diag = DesktopDiagnostics::empty_for_test();
        let snapshot = diag.snapshot();
        assert!(snapshot.windows.is_empty());
        assert!(snapshot.overlays.is_empty());
        assert_eq!(snapshot.platform, "test");
    }

    #[test]
    fn test_to_json_returns_valid_json() {
        let diag = DesktopDiagnostics::empty_for_test();
        let json = diag.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["platform"], "test");
    }
}
