//! # Window Observer
//!
//! Observes ALL windows on the desktop (not just Lumi's own) to enable
//! the character to react to active application changes, window movement,
//! and fullscreen detection.
//!
//! Observation is non-intrusive: the observer never captures, injects
//! into, or modifies other applications' windows.
//!
//! # Thread Safety
//!
//! `WindowObserver` is `Send + Sync` via `DashMap` and `Arc`.

use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, LogicalRect, Point, Rect, Size};
use crate::metrics::DesktopMetrics;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lumas_runtime::event::EventBus;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Platform-specific OS window identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OsWindowId(pub u64);

/// Information about an observed OS window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedWindow {
    /// OS window identifier.
    pub os_id: OsWindowId,
    /// Window title.
    pub title: String,
    /// Application name.
    pub application_name: String,
    /// macOS: bundle identifier.
    pub bundle_id: Option<String>,
    /// OS process ID.
    pub process_id: u32,
    /// Window bounds in logical pixels.
    pub bounds: LogicalRect,
    /// Whether the window is focused.
    pub is_focused: bool,
    /// Whether the window is minimized.
    pub is_minimized: bool,
    /// Whether the window is fullscreen.
    pub is_fullscreen: bool,
    /// When the window was observed.
    pub observed_at: DateTime<Utc>,
}

/// Observes all OS windows for context and event tracking.
pub struct WindowObserver {
    /// All observed windows by OS ID.
    windows: DashMap<OsWindowId, ObservedWindow>,
    /// Event bus for emitting window events.
    event_bus: Arc<EventBus>,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

impl WindowObserver {
    /// Create a new window observer.
    pub fn new(event_bus: Arc<EventBus>, metrics: Arc<DesktopMetrics>) -> Self {
        Self {
            windows: DashMap::new(),
            event_bus,
            metrics,
        }
    }

    /// Returns the currently focused OS window (not Lumi's own windows).
    pub fn focused_window(&self) -> Option<ObservedWindow> {
        self.windows
            .iter()
            .find(|entry| entry.value().is_focused)
            .map(|entry| entry.value().clone())
    }

    /// Returns all currently visible windows sorted by z-order.
    pub fn all_windows(&self) -> Vec<ObservedWindow> {
        let mut windows: Vec<ObservedWindow> = self
            .windows
            .iter()
            .filter(|entry| !entry.value().is_minimized)
            .map(|entry| entry.value().clone())
            .collect();
        windows.sort_by(|a, b| b.observed_at.cmp(&a.observed_at));
        windows
    }

    /// Returns `true` if any application is in fullscreen mode.
    pub fn any_fullscreen(&self) -> bool {
        self.windows
            .iter()
            .any(|entry| entry.value().is_fullscreen)
    }

    /// Returns the foreground application name (for AI context injection).
    pub fn foreground_application(&self) -> Option<String> {
        self.focused_window().map(|w| w.application_name)
    }

    /// Update or insert an observed window. Called by the platform backend.
    pub(crate) fn update_window(&self, window: ObservedWindow) {
        self.windows.insert(window.os_id.clone(), window);
    }

    /// Remove an observed window. Called when it is destroyed.
    pub(crate) fn remove_window(&self, os_id: &OsWindowId) {
        self.windows.remove(os_id);
    }

    /// Start the OS observation loop.
    ///
    /// Platform-specific implementation uses:
    /// - **macOS**: `NSWorkspace.sharedWorkspace.notifications` + `AXObserver`
    /// - **Windows**: `SetWinEventHook` with event constants
    /// - **Linux/X11**: `XRecord` or `_NET_CLIENT_LIST` polling
    /// - **Linux/Wayland**: `ext-foreign-toplevel-list-v1` protocol
    pub async fn start(self: Arc<Self>) -> Result<(), DesktopError> {
        // Platform-specific observation is handled by the backend.
        // This method is a placeholder that signals readiness.
        Ok(())
    }
}

impl std::fmt::Debug for WindowObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowObserver")
            .field("windows_count", &self.windows.len())
            .finish()
    }
}
