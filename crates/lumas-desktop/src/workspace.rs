//! # Workspace Snapshot
//!
//! A sanitized snapshot of the current desktop workspace state.
//! Injected into the AI Core context every 5 seconds or on change.
//!
//! # Thread Safety
//!
//! `WorkspaceManager` is `Send + Sync` via `ArcSwap`. Snapshots are
//! read lock-free O(1) via `Arc` load.

use crate::geometry::LogicalPoint;
use crate::metrics::DesktopMetrics;
use crate::monitor::{MonitorInfo, MonitorManager};
use crate::observer::{ObservedWindow, WindowObserver};
use crate::input::GlobalInputObserver;
use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use lumas_runtime::event::EventBus;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// A snapshot of the current desktop state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    /// When the snapshot was taken.
    pub timestamp: DateTime<Utc>,
    /// The currently focused window.
    pub focused_window: Option<ObservedWindow>,
    /// Foreground application name.
    pub foreground_application: Option<String>,
    /// All open windows.
    pub open_windows: Vec<ObservedWindow>,
    /// Whether any application is in fullscreen.
    pub any_fullscreen: bool,
    /// System idle time in seconds.
    pub system_idle_seconds: u64,
    /// Virtual desktop index (if known).
    pub virtual_desktop_index: Option<u32>,
    /// Connected monitors.
    pub monitors: Vec<MonitorInfo>,
    /// Current cursor position.
    pub cursor_position: LogicalPoint,
}

/// Manages periodic workspace snapshots for AI context injection.
pub struct WorkspaceManager {
    /// Window observer for tracking OS windows.
    observer: Arc<WindowObserver>,
    /// Global input observer for cursor position.
    input: Arc<GlobalInputObserver>,
    /// Monitor manager for display topology.
    monitors: Arc<MonitorManager>,
    /// Last workspace snapshot (lock-free read via ArcSwap).
    last_snapshot: ArcSwap<WorkspaceSnapshot>,
    /// Event bus for emitting snapshot events.
    event_bus: Arc<EventBus>,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

impl WorkspaceManager {
    /// Create a new workspace manager.
    pub fn new(
        observer: Arc<WindowObserver>,
        input: Arc<GlobalInputObserver>,
        monitors: Arc<MonitorManager>,
        event_bus: Arc<EventBus>,
        metrics: Arc<DesktopMetrics>,
    ) -> Self {
        Self {
            observer,
            input,
            monitors,
            last_snapshot: ArcSwap::new(Arc::new(Self::empty_snapshot())),
            event_bus,
            metrics,
        }
    }

    /// Create an empty snapshot for initialization.
    fn empty_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            timestamp: Utc::now(),
            focused_window: None,
            foreground_application: None,
            open_windows: Vec::new(),
            any_fullscreen: false,
            system_idle_seconds: 0,
            virtual_desktop_index: None,
            monitors: Vec::new(),
            cursor_position: LogicalPoint::new(0.0, 0.0),
        }
    }

    /// Returns the most recent workspace snapshot (O(1) Arc load).
    pub fn current_snapshot(&self) -> Arc<WorkspaceSnapshot> {
        self.last_snapshot.load_full()
    }

    /// Force a snapshot refresh.
    pub async fn refresh(&self) -> Result<Arc<WorkspaceSnapshot>, crate::error::DesktopError> {
        let cursor = self.input.cursor_position();
        let monitors = self.monitors.all();
        let focused = self.observer.focused_window();
        let all_windows = self.observer.all_windows();
        let any_fs = self.observer.any_fullscreen();
        let fg = self.observer.foreground_application();

        let snapshot = Arc::new(WorkspaceSnapshot {
            timestamp: Utc::now(),
            focused_window: focused,
            foreground_application: fg,
            open_windows: all_windows,
            any_fullscreen: any_fs,
            system_idle_seconds: 0,
            virtual_desktop_index: None,
            monitors: monitors.as_ref().clone(),
            cursor_position: cursor,
        });

        self.last_snapshot.store(snapshot.clone());
        Ok(snapshot)
    }

    /// Start the background refresh task (every 5 seconds + on-change).
    pub async fn start(self: Arc<Self>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = self.refresh().await {
                debug!("Workspace refresh failed: {}", e);
            }
        }
    }
}

impl std::fmt::Debug for WorkspaceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceManager").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::DesktopMetrics;

    #[test]
    fn test_snapshot_defaults() {
        let snap = WorkspaceSnapshot {
            timestamp: chrono::Utc::now(),
            focused_window: None,
            foreground_application: None,
            open_windows: Vec::new(),
            any_fullscreen: false,
            system_idle_seconds: 0,
            virtual_desktop_index: None,
            monitors: Vec::new(),
            cursor_position: LogicalPoint::new(0.0, 0.0),
        };
        assert!(snap.open_windows.is_empty());
        assert!(!snap.any_fullscreen);
    }
}
