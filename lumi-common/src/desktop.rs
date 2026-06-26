//! # Desktop Awareness — Desktop Context Types (Chapter 15)
//!
//! Defines desktop snapshot structures, window information,
//! focus detection signals, and privacy tiers.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Desktop Snapshot
// ---------------------------------------------------------------------------

/// A lightweight snapshot of the current desktop state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    pub timestamp: i64,
    pub active_window: WindowInfo,
    /// List of all open windows (non-minimized).
    pub open_windows: Vec<WindowEntry>,
    pub user_activity: UserActivity,
    pub system: SystemInfo,
    /// Recent notifications from other applications.
    pub recent_notifications: Vec<NotificationInfo>,
}

/// Information about a specific window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub application: String,
    pub bundle_id: Option<String>,
    pub bounds: Option<WindowBounds>,
    pub pid: Option<u32>,
}

/// Lightweight window entry for the open_windows list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    pub title: String,
    pub application: String,
    pub minimized: bool,
}

/// Screen-space bounds of a window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// User activity state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivity {
    /// Seconds since last user input.
    pub idle_seconds: u64,
    /// Whether focus/do-not-disturb mode is active.
    pub focus_mode_active: bool,
    pub last_input_type: InputType,
}

/// Type of the last user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputType {
    Keyboard,
    Mouse,
    Touch,
    None,
}

/// System resource information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub battery_percent: Option<f32>,
    pub network_connected: bool,
}

/// A notification from another application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationInfo {
    pub application: String,
    pub title: String,
    pub body: Option<String>,
    pub received_at: i64,
}

// ---------------------------------------------------------------------------
// Focus Detection
// ---------------------------------------------------------------------------

/// Signals that contribute to focus/do-not-disturb detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FocusSignal {
    /// OS-level Do Not Disturb is enabled.
    SystemDoNotDisturb,
    /// A full-screen application is active.
    FullscreenApplication,
    /// User has been actively typing for a sustained period.
    UserIdleBelow { seconds: u32 },
    /// User manually toggled Lumi quiet mode.
    ManualFocusToggle,
    /// Focus mode is active via multiple signals.
    FocusActive,
    /// Focus mode has ended.
    FocusEnded,
}

/// Configuration for the focus detector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusDetectorConfig {
    /// Seconds of inactivity before considering idle.
    pub idle_threshold_seconds: u64,
    /// Seconds of sustained typing before considering focus.
    pub typing_focus_seconds: u32,
}

impl Default for FocusDetectorConfig {
    fn default() -> Self {
        Self {
            idle_threshold_seconds: 300, // 5 minutes
            typing_focus_seconds: 120,   // 2 minutes
        }
    }
}

// ---------------------------------------------------------------------------
// Privacy Tiers
// ---------------------------------------------------------------------------

/// Privacy sensitivity tiers for desktop awareness data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyTier {
    /// Window titles and app names — always available.
    Low,
    /// Notification content — requires medium trust.
    Medium,
    /// Clipboard content — requires explicit user request.
    High,
    /// Screen pixel data — requires explicit user consent per capture.
    VeryHigh,
}

/// A request to capture a screen region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCaptureRequest {
    pub region: Option<WindowBounds>,
    pub reason: String,
    pub consent_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Desktop Events
// ---------------------------------------------------------------------------

/// Events emitted by the Desktop Awareness subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DesktopEvent {
    /// The active foreground window changed.
    ActiveWindowChanged(WindowInfo),
    /// User input was detected (keyboard, mouse, touch).
    UserInputDetected(InputType),
    /// User has become idle.
    UserIdle,
    /// User has returned from idle.
    UserActive,
    /// A notification was received.
    NotificationReceived(NotificationInfo),
    /// Monitor configuration changed.
    MonitorConfigurationChanged,
    /// DPI scaling factor changed.
    DPIScaleChanged { new_scale: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_snapshot_creation() {
        let snapshot = DesktopSnapshot {
            timestamp: chrono::Utc::now().timestamp(),
            active_window: WindowInfo {
                title: "VSCode".into(),
                application: "Visual Studio Code".into(),
                bundle_id: Some("com.microsoft.VSCode".into()),
                bounds: Some(WindowBounds { x: 0.0, y: 0.0, width: 1200.0, height: 800.0 }),
                pid: Some(12345),
            },
            open_windows: vec![],
            user_activity: UserActivity {
                idle_seconds: 0,
                focus_mode_active: false,
                last_input_type: InputType::Keyboard,
            },
            system: SystemInfo {
                cpu_percent: 15.0,
                memory_percent: 65.0,
                battery_percent: Some(80.0),
                network_connected: true,
            },
            recent_notifications: vec![],
        };

        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["active_window"]["title"], "VSCode");
        assert_eq!(json["system"]["cpu_percent"], 15.0);
    }

    #[test]
    fn test_privacy_tier_ordering() {
        assert!(PrivacyTier::Low as u8 < PrivacyTier::Medium as u8);
        assert!(PrivacyTier::Medium as u8 < PrivacyTier::High as u8);
        assert!(PrivacyTier::High as u8 < PrivacyTier::VeryHigh as u8);
    }

    #[test]
    fn test_focus_detector_config_default() {
        let config = FocusDetectorConfig::default();
        assert_eq!(config.idle_threshold_seconds, 300);
        assert_eq!(config.typing_focus_seconds, 120);
    }
}
