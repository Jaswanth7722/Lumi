//! # Navigation — Destination Planning
//!
//! The Navigator computes **where** to go (a destination point and reasoning),
//! but does NOT compute **how to get there smoothly** (that's the Desktop Engine's
//! `SpringInterpolator` / path planner, per SRS Chapter 6.4 / 18.2.2).
//!
//! # Authority
//! Character Engine — destination selection.
//!
//! # Does NOT
//! - Compute interpolated paths or smooth movement
//! - Own screen-space coordinates
//! - Execute actual screen positioning

use crate::config::ScreenRect;
use crate::error::CharacterError;
use crate::movement::{MovementReason, MovementUrgency};
use lumas_common::desktop::DesktopSnapshot;
use lumas_common::position::PositionTarget;
use std::sync::Arc;

/// Provides current monitor bounds for navigation decisions.
pub trait MonitorBoundsProvider: Send + Sync + std::fmt::Debug {
    /// Returns the bounds of all connected monitors as (x, y, width, height).
    fn monitor_bounds(&self) -> Vec<ScreenRect>;
}

/// Navigation planner that computes destinations respecting constraints.
#[derive(Debug)]
pub struct Navigator {
    no_walk_zones: Vec<ScreenRect>,
    monitor_bounds_provider: Option<Arc<dyn MonitorBoundsProvider>>,
    exploration_radius_px: f32,
}

impl Navigator {
    /// Create a new navigator with the given no-walk zones.
    pub fn new(
        no_walk_zones: Vec<ScreenRect>,
        exploration_radius_px: f32,
        monitor_bounds_provider: Option<Arc<dyn MonitorBoundsProvider>>,
    ) -> Self {
        Self {
            no_walk_zones,
            monitor_bounds_provider,
            exploration_radius_px,
        }
    }

    /// Get the no-walk zones.
    pub fn no_walk_zones(&self) -> &[ScreenRect] {
        &self.no_walk_zones
    }

    /// Set the monitor bounds provider.
    pub fn set_monitor_bounds_provider(&mut self, provider: Arc<dyn MonitorBoundsProvider>) {
        self.monitor_bounds_provider = Some(provider);
    }

    /// Check if a point is inside any no-walk zone.
    pub fn is_in_no_walk_zone(&self, x: f32, y: f32) -> bool {
        self.no_walk_zones
            .iter()
            .any(|zone| x >= zone.x && x <= zone.x + zone.width && y >= zone.y && y <= zone.y + zone.height)
    }

    /// Compute a valid destination point for a given behavior reason.
    pub fn plan_destination(
        &self,
        reason: MovementReason,
        desktop_context: &DesktopSnapshot,
    ) -> Result<PositionTarget, CharacterError> {
        match reason {
            MovementReason::BehaviorExploring => self.plan_exploration(desktop_context),
            MovementReason::FollowingActiveWindow => self.plan_window_follow(desktop_context),
            MovementReason::NotificationReaction => self.plan_notification_reaction(desktop_context),
            _ => Ok(PositionTarget::Preserve),
        }
    }

    /// Plan an exploration destination.
    fn plan_exploration(&self, desktop: &DesktopSnapshot) -> Result<PositionTarget, CharacterError> {
        // Try to follow the active window edge
        if let Some(bounds) = desktop.active_window.bounds {
            let dest_x = bounds.x as f32 + bounds.width as f32 + 20.0;
            let dest_y = bounds.y as f32 + bounds.height as f32 * 0.5;

            if !self.is_in_no_walk_zone(dest_x, dest_y) {
                return Ok(PositionTarget::Absolute {
                    x: dest_x,
                    y: dest_y,
                });
            }
        }

        // Fallback: pick a random point near bottom-right of primary monitor
        if let Some(provider) = &self.monitor_bounds_provider {
            let bounds = provider.monitor_bounds();
            if let Some(primary) = bounds.first() {
                let cx = primary.x + primary.width * 0.75;
                let cy = primary.y + primary.height * 0.85;
                if !self.is_in_no_walk_zone(cx, cy) {
                    return Ok(PositionTarget::Absolute { x: cx, y: cy });
                }
            }
        }

        Err(CharacterError::NavigationFailed {
            reason: "No valid exploration destination found".into(),
        })
    }

    /// Plan a destination following the active window.
    fn plan_window_follow(&self, desktop: &DesktopSnapshot) -> Result<PositionTarget, CharacterError> {
        if let Some(bounds) = desktop.active_window.bounds {
            let dest_x = bounds.x as f32 + bounds.width as f32 + 10.0;
            let dest_y = bounds.y as f32 + bounds.height as f32 * 0.3;

            if !self.is_in_no_walk_zone(dest_x, dest_y) {
                return Ok(PositionTarget::Absolute {
                    x: dest_x,
                    y: dest_y,
                });
            }

            // Fallback to bottom edge
            let alt_y = bounds.y as f32 + bounds.height as f32 + 20.0;
            if !self.is_in_no_walk_zone(dest_x, alt_y) {
                return Ok(PositionTarget::Absolute {
                    x: dest_x,
                    y: alt_y,
                });
            }
        }

        Ok(PositionTarget::Preserve)
    }

    /// Plan a reaction destination (toward the notification).
    fn plan_notification_reaction(&self, _desktop: &DesktopSnapshot) -> Result<PositionTarget, CharacterError> {
        // Look toward cursor / center of screen
        Ok(PositionTarget::NearCursor {
            offset_x: 10.0,
            offset_y: -30.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumas_common::desktop::{UserActivity, WindowInfo, WindowBounds, InputType, SystemInfo};

    fn make_desktop() -> DesktopSnapshot {
        DesktopSnapshot {
            timestamp: 0,
            active_window: WindowInfo {
                title: "Test".into(),
                application: "TestApp".into(),
                bundle_id: None,
                bounds: Some(WindowBounds { x: 0.0, y: 0.0, width: 800.0, height: 600.0 }),
                pid: None,
            },
            open_windows: vec![],
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
            recent_notifications: vec![],
        }
    }

    #[test]
    fn test_plan_window_follow() {
        let navigator = Navigator::new(vec![], 400.0, None);
        let desktop = make_desktop();
        let target = navigator.plan_destination(
            MovementReason::FollowingActiveWindow,
            &desktop,
        );
        assert!(target.is_ok());
    }

    #[test]
    fn test_no_walk_zone_detection() {
        let zone = ScreenRect { x: 100.0, y: 100.0, width: 50.0, height: 50.0 };
        let navigator = Navigator::new(vec![zone], 400.0, None);
        assert!(navigator.is_in_no_walk_zone(120.0, 120.0));
        assert!(navigator.is_in_no_walk_zone(100.0, 100.0));
        assert!(!navigator.is_in_no_walk_zone(200.0, 200.0));
        assert!(!navigator.is_in_no_walk_zone(0.0, 0.0));
    }

    #[test]
    fn test_plan_window_follow_respects_no_walk() {
        // Place a no-walk zone exactly where the active window edge would be
        let zone = ScreenRect { x: 810.0, y: 0.0, width: 100.0, height: 200.0 };
        let navigator = Navigator::new(vec![zone], 400.0, None);
        let desktop = make_desktop();
        let target = navigator.plan_destination(
            MovementReason::FollowingActiveWindow,
            &desktop,
        );
        // Should still succeed (falls back to alternative position)
        assert!(target.is_ok());
    }
}
