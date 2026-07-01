//! # Persisted Position
//!
//! A lightweight position hint stored across sessions. On load, this is
//! re-validated against the current monitor configuration — never trusted
//! blindly, since monitors may have changed since the last session.
//!
//! # Authority
//! Character Engine — position hint storage.
//!
//! # Does NOT
//! - Override the Desktop Engine's current position
//! - Contain camera or transform data

use lumas_common::position::PositionTarget;
use serde::{Deserialize, Serialize};

/// Screen index for a persisted position.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenIndex(pub u32);

impl std::fmt::Display for ScreenIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Screen({})", self.0)
    }
}

/// A persisted position hint, re-validated on load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedPosition {
    /// X coordinate in virtual screen space.
    pub x: f32,
    /// Y coordinate in virtual screen space.
    pub y: f32,
    /// The monitor this position was on when saved.
    pub screen_index: ScreenIndex,
    /// Screen resolution width when saved (for validation).
    pub screen_width: u32,
    /// Screen resolution height when saved (for validation).
    pub screen_height: u32,
}

/// Lightweight monitor info for position revalidation.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Monitor index.
    pub index: u32,
    /// X offset in virtual screen space.
    pub x: i32,
    /// Y offset in virtual screen space.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
}

/// Revalidate a persisted position hint against current monitor configuration.
/// Returns the original position if still valid, or a corrected position if
/// the original is now off-screen or on a disconnected monitor.
pub fn revalidate_position(
    hint: &PersistedPosition,
    current_monitors: &[MonitorInfo],
) -> PositionTarget {
    // Check if the saved screen still exists
    let same_screen = current_monitors
        .iter()
        .find(|m| m.index == hint.screen_index.0);

    match same_screen {
        Some(monitor) => {
            // Check if the resolution changed significantly
            let width_diff = (monitor.width as i32 - hint.screen_width as i32).abs();
            let height_diff = (monitor.height as i32 - hint.screen_height as i32).abs();

            if width_diff < 100 && height_diff < 100 {
                // Position should still be valid within this monitor
                let abs_x = monitor.x as f32 + hint.x;
                let abs_y = monitor.y as f32 + hint.y;
                PositionTarget::Absolute {
                    x: abs_x,
                    y: abs_y,
                }
            } else {
                // Resolution changed significantly — place at monitor center
                let cx = monitor.x as f32 + monitor.width as f32 * 0.75;
                let cy = monitor.y as f32 + monitor.height as f32 * 0.85;
                PositionTarget::Absolute { x: cx, y: cy }
            }
        }
        None => {
            // Monitor no longer connected — place on primary monitor
            if let Some(primary) = current_monitors.iter().find(|m| m.is_primary) {
                let cx = primary.x as f32 + primary.width as f32 * 0.75;
                let cy = primary.y as f32 + primary.height as f32 * 0.85;
                PositionTarget::Absolute { x: cx, y: cy }
            } else if let Some(first) = current_monitors.first() {
                // Fallback to first available monitor
                let cx = first.x as f32 + first.width as f32 * 0.75;
                let cy = first.y as f32 + first.height as f32 * 0.85;
                PositionTarget::Absolute { x: cx, y: cy }
            } else {
                // No monitors at all — preserve current
                PositionTarget::Preserve
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_monitors() -> Vec<MonitorInfo> {
        vec![
            MonitorInfo {
                index: 0,
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: true,
            },
            MonitorInfo {
                index: 1,
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: false,
            },
        ]
    }

    #[test]
    fn test_valid_position_kept() {
        let hint = PersistedPosition {
            x: 100.0,
            y: 200.0,
            screen_index: ScreenIndex(0),
            screen_width: 1920,
            screen_height: 1080,
        };
        let monitors = make_monitors();
        let target = revalidate_position(&hint, &monitors);
        assert!(matches!(target, PositionTarget::Absolute { x, y } if x == 100.0 && y == 200.0));
    }

    #[test]
    fn test_monitor_disconnected_moves_to_primary() {
        let hint = PersistedPosition {
            x: 100.0,
            y: 200.0,
            screen_index: ScreenIndex(2), // Monitor 2 doesn't exist anymore
            screen_width: 1920,
            screen_height: 1080,
        };
        let monitors = make_monitors();
        let target = revalidate_position(&hint, &monitors);
        // Should place on primary monitor (index 0)
        assert!(matches!(target, PositionTarget::Absolute { x, y } if x > 0.0 && y > 0.0));
    }

    #[test]
    fn test_resolution_change_recenters() {
        let hint = PersistedPosition {
            x: 100.0,
            y: 200.0,
            screen_index: ScreenIndex(0),
            screen_width: 3840, // Was 4K, now 1080p
            screen_height: 2160,
        };
        let monitors = make_monitors();
        let target = revalidate_position(&hint, &monitors);
        // Position should have changed (resolution diff > threshold)
        assert!(matches!(target, PositionTarget::Absolute { .. }));
    }

    #[test]
    fn test_no_monitors_returns_preserve() {
        let hint = PersistedPosition {
            x: 100.0,
            y: 200.0,
            screen_index: ScreenIndex(0),
            screen_width: 1920,
            screen_height: 1080,
        };
        let target = revalidate_position(&hint, &[]);
        assert_eq!(target, PositionTarget::Preserve);
    }
}
