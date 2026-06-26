//! # Desktop Engine — Positioning System (Chapter 6)
//!
//! Defines the positioning targets, window anchoring options,
//! spring interpolator, and hit testing types for the Desktop Engine.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Position Targets
// ---------------------------------------------------------------------------

/// Where Lumi should move to on the desktop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PositionTarget {
    /// Move to an absolute screen position.
    Absolute { x: f32, y: f32 },
    /// Position relative to a specific window.
    RelativeToWindow {
        window_id: String,
        anchor: WindowAnchor,
    },
    /// Position near the cursor with an offset.
    NearCursor { offset_x: f32, offset_y: f32 },
    /// Position at a screen edge.
    ScreenEdge { edge: ScreenEdge, position: f32 },
    /// Stay in the current position.
    Preserve,
}

/// Anchoring position relative to a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowAnchor {
    BottomLeft,
    BottomRight,
    BottomCenter,
    LeftSide,
    RightSide,
}

/// A screen edge for positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenEdge {
    Left,
    Right,
    Top,
    Bottom,
}

// ---------------------------------------------------------------------------
// Spring Interpolator
// ---------------------------------------------------------------------------

/// Critically-damped spring interpolation for smooth position transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringInterpolator {
    /// Spring stiffness (default 120.0).
    pub stiffness: f32,
    /// Damping coefficient (default 20.0).
    pub damping: f32,
    /// Mass (default 1.0).
    pub mass: f32,
    /// Current velocity in pixels/second.
    pub velocity: (f32, f32),
    /// Current position.
    pub current: (f32, f32),
    /// Target position.
    pub target: (f32, f32),
}

impl SpringInterpolator {
    /// Create a new spring interpolator at a given position.
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            stiffness: 120.0,
            damping: 20.0,
            mass: 1.0,
            velocity: (0.0, 0.0),
            current: (x, y),
            target: (x, y),
        }
    }

    /// Configure the spring parameters.
    pub fn with_params(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self {
            stiffness,
            damping,
            mass,
            velocity: (0.0, 0.0),
            current: (0.0, 0.0),
            target: (0.0, 0.0),
        }
    }

    /// Set a new target position.
    pub fn set_target(&mut self, x: f32, y: f32) {
        self.target = (x, y);
    }

    /// Update the spring for the given time step.
    /// Returns the new position.
    pub fn update(&mut self, dt: f32) -> (f32, f32) {
        let force_x =
            (self.target.0 - self.current.0) * self.stiffness - self.velocity.0 * self.damping;
        let force_y =
            (self.target.1 - self.current.1) * self.stiffness - self.velocity.1 * self.damping;

        self.velocity.0 += force_x / self.mass * dt;
        self.velocity.1 += force_y / self.mass * dt;

        self.current.0 += self.velocity.0 * dt;
        self.current.1 += self.velocity.1 * dt;

        self.current
    }

    /// Check if the spring is at rest (close to target with low velocity).
    pub fn is_at_rest(&self, epsilon: f32) -> bool {
        let dx = self.current.0 - self.target.0;
        let dy = self.current.1 - self.target.1;
        let dist_sq = dx * dx + dy * dy;
        let speed_sq = self.velocity.0 * self.velocity.0 + self.velocity.1 * self.velocity.1;
        dist_sq < epsilon && speed_sq < epsilon
    }
}

// ---------------------------------------------------------------------------
// Hit Testing
// ---------------------------------------------------------------------------

/// Result of a hit test against the character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HitResult {
    /// The point hit the character.
    Hit { alpha: u8 },
    /// The point missed the character.
    Miss,
}

/// Configuration for the hit tester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitTesterConfig {
    /// Alpha threshold for hit detection (0-255, default 64 = 25%).
    pub hit_threshold: u8,
}

impl Default for HitTesterConfig {
    fn default() -> Self {
        Self { hit_threshold: 64 }
    }
}

/// Size of the alpha mask in pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaMaskSize {
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_interpolator_reaches_target() {
        let mut spring = SpringInterpolator::new(0.0, 0.0);
        spring.set_target(100.0, 200.0);

        // Simulate 60 FPS for 2 seconds (120 frames)
        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            spring.update(dt);
        }

        // Should be very close to target
        let (x, y) = spring.current;
        assert!((x - 100.0).abs() < 1.0);
        assert!((y - 200.0).abs() < 1.0);
        assert!(spring.is_at_rest(1.0));
    }

    #[test]
    fn test_spring_starts_at_rest() {
        let spring = SpringInterpolator::new(50.0, 50.0);
        assert!(spring.is_at_rest(1.0));
    }

    #[test]
    fn test_hit_tester_config_default() {
        let config = HitTesterConfig::default();
        assert_eq!(config.hit_threshold, 64);
    }

    #[test]
    fn test_position_target_variants() {
        let targets = vec![
            PositionTarget::Absolute { x: 100.0, y: 200.0 },
            PositionTarget::NearCursor {
                offset_x: 20.0,
                offset_y: 20.0,
            },
            PositionTarget::Preserve,
            PositionTarget::ScreenEdge {
                edge: ScreenEdge::Bottom,
                position: 0.5,
            },
        ];
        for target in targets {
            let json = serde_json::to_value(&target).unwrap();
            let back: PositionTarget = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{target:?}"), format!("{back:?}"));
        }
    }

    #[test]
    fn test_window_anchor_positions() {
        assert_eq!(WindowAnchor::BottomLeft as u8, 0);
        assert_eq!(WindowAnchor::RightSide as u8, 4);
    }
}
