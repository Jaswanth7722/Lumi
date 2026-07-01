//! # Desktop Awareness — Desktop Context Monitoring (Chapter 15)
//!
//! Monitors active windows, user activity, system notifications,
//! and provides desktop snapshots for AI context injection.

use lumas_common::desktop::{
    DesktopEvent, DesktopSnapshot, FocusDetectorConfig, InputType,
    SystemInfo, UserActivity, WindowInfo,
};
use tracing::debug;

/// Desktop Awareness subsystem providing real-time desktop context.
pub struct DesktopAwareness {
    /// Current desktop snapshot.
    snapshot: DesktopSnapshot,
    /// Focus detector configuration.
    focus_config: FocusDetectorConfig,
    /// Whether focus mode is active.
    focus_mode: bool,
    /// Recent desktop events.
    recent_events: Vec<DesktopEvent>,
}

impl DesktopAwareness {
    pub fn new() -> Self {
        Self {
            snapshot: DesktopSnapshot {
                timestamp: chrono::Utc::now().timestamp_millis(),
                active_window: WindowInfo {
                    title: String::new(),
                    application: String::new(),
                    bundle_id: None,
                    bounds: None,
                    pid: None,
                },
                open_windows: Vec::new(),
                user_activity: UserActivity {
                    idle_seconds: 0,
                    focus_mode_active: false,
                    last_input_type: InputType::None,
                },
                system: SystemInfo {
                    cpu_percent: 0.0,
                    memory_percent: 0.0,
                    battery_percent: None,
                    network_connected: true,
                },
                recent_notifications: Vec::new(),
            },
            focus_config: FocusDetectorConfig::default(),
            focus_mode: false,
            recent_events: Vec::new(),
        }
    }

    /// Update the desktop snapshot (called periodically).
    pub async fn update_snapshot(&mut self) {
        // In production, this would query OS APIs.
        // For the skeleton, just update the timestamp.
        self.snapshot.timestamp = chrono::Utc::now().timestamp_millis();

        // Simulate idle tracking
        if self.snapshot.user_activity.idle_seconds > 0 {
            self.snapshot.user_activity.idle_seconds += 1;
        }

        // Check focus mode
        self.update_focus_mode().await;
    }

    /// Check and update focus/do-not-disturb mode.
    async fn update_focus_mode(&mut self) {
        let was_focus = self.focus_mode;
        self.focus_mode = self.check_focus_signals();

        self.snapshot.user_activity.focus_mode_active = self.focus_mode;

        if was_focus != self.focus_mode {
            if self.focus_mode {
                debug!("Focus mode activated");
                self.recent_events
                    .push(DesktopEvent::UserInputDetected(InputType::None));
            } else {
                debug!("Focus mode deactivated");
            }
        }
    }

    /// Check all focus signals.
    fn check_focus_signals(&self) -> bool {
        false // In production, check system DND, fullscreen, etc.
    }

    /// Register a user input event.
    pub fn register_input(&mut self, input_type: InputType) {
        self.snapshot.user_activity.last_input_type = input_type;
        self.snapshot.user_activity.idle_seconds = 0;
        self.recent_events
            .push(DesktopEvent::UserInputDetected(input_type));
    }

    /// Register an idle event.
    pub fn register_idle(&mut self) {
        self.snapshot.user_activity.idle_seconds = self.focus_config.idle_threshold_seconds as u64;
        self.recent_events.push(DesktopEvent::UserIdle);
    }

    /// Get the current desktop snapshot.
    pub fn get_snapshot(&self) -> &DesktopSnapshot {
        &self.snapshot
    }

    /// Get the current desktop snapshot as a sanitized JSON value for AI context.
    pub fn get_snapshot_for_context(&self) -> serde_json::Value {
        serde_json::json!({
            "active_window": {
                "title": self.snapshot.active_window.title,
                "application": self.snapshot.active_window.application,
            },
            "user_activity": {
                "idle_seconds": self.snapshot.user_activity.idle_seconds,
                "focus_mode": self.snapshot.user_activity.focus_mode_active,
            },
            "system": {
                "cpu_percent": self.snapshot.system.cpu_percent,
                "memory_percent": self.snapshot.system.memory_percent,
            },
        })
    }

    /// Set the active window.
    pub fn set_active_window(&mut self, window: WindowInfo) {
        self.snapshot.active_window = window;
    }

    /// Check if focus mode is active.
    pub fn is_focus_mode(&self) -> bool {
        self.focus_mode
    }

    /// Get the idle seconds.
    pub fn idle_seconds(&self) -> u64 {
        self.snapshot.user_activity.idle_seconds
    }

    /// Drain recent desktop events.
    pub fn drain_events(&mut self) -> Vec<DesktopEvent> {
        self.recent_events.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let awareness = DesktopAwareness::new();
        assert!(!awareness.is_focus_mode());
        assert_eq!(awareness.idle_seconds(), 0);
    }

    #[test]
    fn test_register_input_resets_idle() {
        let mut awareness = DesktopAwareness::new();
        awareness.register_input(InputType::Keyboard);
        assert_eq!(awareness.idle_seconds(), 0);
        assert_eq!(
            awareness.snapshot.user_activity.last_input_type,
            InputType::Keyboard
        );
    }

    #[test]
    fn test_snapshot_for_context() {
        let awareness = DesktopAwareness::new();
        let context = awareness.get_snapshot_for_context();
        assert!(context.get("active_window").is_some());
        assert!(context.get("user_activity").is_some());
        assert!(context.get("system").is_some());
    }

    #[test]
    fn test_drain_events() {
        let mut awareness = DesktopAwareness::new();
        awareness.register_input(InputType::Mouse);
        let events = awareness.drain_events();
        assert_eq!(events.len(), 1);
        assert!(awareness.drain_events().is_empty());
    }
}
