//! # Desktop Engine — Desktop Window Management (Chapter 6)
//!
//! Manages the transparent, always-on-top window, hit testing,
//! positioning system, and platform-specific window integration.

use lumas_common::position::{
    AlphaMaskSize, HitResult, HitTesterConfig, PositionTarget, SpringInterpolator, WindowAnchor,
};
use tracing::debug;

/// The Desktop Engine manages Lumi's desktop window presence.
pub struct DesktopEngine {
    /// Spring interpolator for smooth position transitions.
    spring: SpringInterpolator,
    /// Current character bounds on screen.
    bounds: (f32, f32, f32, f32), // x, y, width, height
    /// Hit testing configuration.
    hit_tester_config: HitTesterConfig,
    /// Whether mouse events pass through the window.
    mouse_passthrough: bool,
    /// The alpha mask size.
    mask_size: AlphaMaskSize,
    /// Whether the window has been created.
    window_created: bool,
}

impl DesktopEngine {
    pub fn new() -> Self {
        Self {
            spring: SpringInterpolator::new(500.0, 500.0),
            bounds: (500.0, 500.0, 100.0, 150.0),
            hit_tester_config: HitTesterConfig::default(),
            mouse_passthrough: true,
            mask_size: AlphaMaskSize {
                width: 256,
                height: 256,
            },
            window_created: false,
        }
    }

    /// Initialize the desktop window.
    pub fn create_window(&mut self) {
        // In production, this creates a transparent, borderless, always-on-top window:
        // - macOS: NSWindow with .borderless, .floating, isOpaque = false
        // - Windows: CreateWindowEx with WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST
        // - Linux/X11: _NET_WM_WINDOW_TYPE_DESKTOP, _NET_WM_STATE_ABOVE
        // - Linux/Wayland: wlr-layer-shell-unstable-v1, ZWLR_LAYER_SHELL_V1_LAYER_TOP
        self.window_created = true;
        debug!("Desktop window created");
    }

    /// Move Lumas to a target position on the desktop.
    pub fn move_to(&mut self, target: PositionTarget) {
        match target {
            PositionTarget::Absolute { x, y } => {
                self.spring.set_target(x, y);
                debug!("Moving to absolute position: ({}, {})", x, y);
            }
            PositionTarget::Preserve => {
                // Stay in current position
            }
            PositionTarget::NearCursor { offset_x, offset_y } => {
                // In production, query cursor position and add offset
                self.spring.set_target(
                    self.spring.target.0 + offset_x,
                    self.spring.target.1 + offset_y,
                );
            }
            PositionTarget::RelativeToWindow {
                window_id: _,
                anchor,
            } => {
                // In production, query window bounds and compute position
                match anchor {
                    WindowAnchor::BottomLeft => {
                        self.spring.set_target(100.0, 100.0);
                    }
                    WindowAnchor::BottomRight => {
                        self.spring.set_target(800.0, 100.0);
                    }
                    _ => {}
                }
            }
            PositionTarget::ScreenEdge { edge: _, position } => {
                // In production, compute edge position based on monitor bounds
                self.spring.set_target(position * 1920.0, 500.0);
            }
        }
    }

    /// Update the desktop engine (called every frame).
    pub fn update(&mut self, dt: f32) {
        let (x, y) = self.spring.update(dt);
        self.bounds.0 = x;
        self.bounds.1 = y;
    }

    /// Test if a screen point hits the character.
    pub fn hit_test(&self, screen_x: i32, screen_y: i32) -> HitResult {
        let (bx, by, bw, bh) = self.bounds;

        if screen_x as f32 >= bx
            && screen_x as f32 <= bx + bw
            && screen_y as f32 >= by
            && screen_y as f32 <= by + bh
        {
            // In production, check the alpha mask at this point
            HitResult::Hit { alpha: 255 }
        } else {
            HitResult::Miss
        }
    }

    /// Enable or disable mouse passthrough on the window.
    pub fn set_mouse_passthrough(&mut self, enabled: bool) {
        self.mouse_passthrough = enabled;
    }

    /// Get the current window position.
    pub fn position(&self) -> (f32, f32) {
        (self.bounds.0, self.bounds.1)
    }

    /// Get the current window bounds.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        self.bounds
    }

    /// Get the character bounds.
    pub fn character_bounds(&self) -> (f32, f32, f32, f32) {
        (self.bounds.0, self.bounds.1, self.bounds.2, self.bounds.3)
    }

    /// Check if the window has been created.
    pub fn is_window_created(&self) -> bool {
        self.window_created
    }

    /// Check if the spring is at rest.
    pub fn is_at_rest(&self) -> bool {
        self.spring.is_at_rest(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let engine = DesktopEngine::new();
        assert!(!engine.is_window_created());
        assert_eq!(engine.position(), (500.0, 500.0));
    }

    #[test]
    fn test_window_creation() {
        let mut engine = DesktopEngine::new();
        engine.create_window();
        assert!(engine.is_window_created());
    }

    #[test]
    fn test_move_to_absolute() {
        let mut engine = DesktopEngine::new();
        engine.move_to(PositionTarget::Absolute { x: 100.0, y: 200.0 });
        // After one update with dt=1.0, should have moved toward target
        engine.update(1.0);
        let (x, y) = engine.position();
        assert!(x > 500.0 || x < 500.0); // Should have moved
    }

    #[test]
    fn test_hit_test() {
        let engine = DesktopEngine::new();
        // Should hit within bounds
        assert_eq!(engine.hit_test(550, 550), HitResult::Hit { alpha: 255 });
        // Should miss outside bounds
        assert_eq!(engine.hit_test(0, 0), HitResult::Miss);
    }

    #[test]
    fn test_mouse_passthrough() {
        let mut engine = DesktopEngine::new();
        engine.set_mouse_passthrough(false);
        // Check via hit test — in production this toggles WS_EX_TRANSPARENT
        engine.set_mouse_passthrough(true);
    }

    #[test]
    fn test_spring_settles() {
        let mut engine = DesktopEngine::new();
        engine.move_to(PositionTarget::Absolute { x: 500.0, y: 500.0 });
        // Already at 500,500 — should be at rest
        for _ in 0..60 {
            engine.update(1.0 / 60.0);
        }
        assert!(engine.is_at_rest());
    }
}
