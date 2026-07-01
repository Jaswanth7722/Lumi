//! Configuration for the Desktop Engine.
//!
//! Mirrors the fields from `lumas-config::schema::rendering::RenderingConfig`
//! with additional desktop-specific options.
//!
//! # Thread Safety
//! `DesktopConfig` is `Send + Sync`. It can be stored in an `ArcSwap` for
//! lock-free runtime updates.

use crate::geometry::LogicalSize;
use crate::monitor::MonitorId;

/// Configuration for the Desktop Engine.
///
/// Provides default values for all window creation parameters, DPI handling,
/// and performance tuning.
///
/// # Examples
/// ```
/// use lumas_desktop::config::DesktopConfig;
/// let config = DesktopConfig::default();
/// assert!(config.stage_width > 0.0);
/// assert!(config.hit_threshold <= 255);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DesktopConfig {
    // --- Stage Window ---
    /// Default stage window width in logical pixels.
    pub stage_width: f64,
    /// Default stage window height in logical pixels.
    pub stage_height: f64,
    /// Initial horizontal offset from the screen edge (in logical pixels).
    pub stage_margin_x: f64,
    /// Initial vertical offset from the screen edge (in logical pixels).
    pub stage_margin_y: f64,
    /// Screen edge to which the stage window snaps by default.
    pub stage_default_anchor: AnchorEdge,

    // --- Hit Testing ---
    /// Minimum alpha value (0–255) for a pixel to be considered interactive.
    pub hit_threshold: u8,
    /// Maximum time (µs) the hit tester should spend per test before
    /// falling back to the bounding box pre-check.
    pub hit_test_max_us: u64,

    // --- Workspace Panels ---
    /// Default width for workspace panels in logical pixels.
    pub panel_width: f64,
    /// Default height for workspace panels in logical pixels.
    pub panel_height: f64,
    /// Default opacity for workspace panels (0.0–1.0).
    pub panel_opacity: f32,
    /// Corner radius for workspace panels in logical pixels.
    pub panel_corner_radius: f32,

    // --- Performance ---
    /// Maximum frames per second for the event loop (0 = unlimited).
    pub max_fps: u32,
    /// Whether to enable vsync for the stage window.
    pub vsync_enabled: bool,

    // --- Observers ---
    /// Whether to start the global input observer on initialization.
    pub enable_input_observer: bool,
    /// Whether to start the OS window observer on initialization.
    pub enable_window_observer: bool,
    /// Whether to enable the drag-drop target system.
    pub enable_drag_drop: bool,

    // --- Behavior ---
    /// Default shutdown timeout for window destruction in milliseconds.
    pub window_shutdown_timeout_ms: u64,
    /// Default command timeout for desktop channel commands in milliseconds.
    pub command_timeout_ms: u64,
    /// Whether to automatically enforce the z-order tier system.
    pub auto_enforce_z_order: bool,

    // --- Diagnostics ---
    /// Interval (seconds) between automatic diagnostics snapshots.
    pub diagnostics_interval_secs: u64,
}

/// Screen edge for initial stage window anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AnchorEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            // Stage defaults: 400x600, bottom-right corner.
            stage_width: 400.0,
            stage_height: 600.0,
            stage_margin_x: 20.0,
            stage_margin_y: 20.0,
            stage_default_anchor: AnchorEdge::BottomRight,

            // Hit testing: 25% opacity threshold, 50µs budget.
            hit_threshold: 64,
            hit_test_max_us: 50,

            // Panel defaults: 480x640, 88% opacity, 12px radius.
            panel_width: 480.0,
            panel_height: 640.0,
            panel_opacity: 0.88,
            panel_corner_radius: 12.0,

            // Performance: 60 FPS, vsync enabled.
            max_fps: 60,
            vsync_enabled: true,

            // Observers: all enabled by default.
            enable_input_observer: true,
            enable_window_observer: true,
            enable_drag_drop: true,

            // Timeouts: 5s for window shutdown, 2s for commands.
            window_shutdown_timeout_ms: 5_000,
            command_timeout_ms: 2_000,
            auto_enforce_z_order: true,

            // Diagnostics snapshot every 30 seconds.
            diagnostics_interval_secs: 30,
        }
    }
}

/// Builder for `DesktopConfig` with ergonomic overrides.
#[derive(Debug)]
pub struct DesktopConfigBuilder {
    config: DesktopConfig,
}

impl DesktopConfigBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: DesktopConfig::default(),
        }
    }

    /// Set the stage window size.
    pub fn stage_size(mut self, width: f64, height: f64) -> Self {
        self.config.stage_width = width;
        self.config.stage_height = height;
        self
    }

    /// Set the hit test threshold.
    pub fn hit_threshold(mut self, threshold: u8) -> Self {
        self.config.hit_threshold = threshold;
        self
    }

    /// Set the maximum FPS.
    pub fn max_fps(mut self, fps: u32) -> Self {
        self.config.max_fps = fps;
        self
    }

    /// Disable input observation (privacy-sensitive environments).
    pub fn disable_input_observer(mut self) -> Self {
        self.config.enable_input_observer = false;
        self
    }

    /// Disable window observation (privacy-sensitive environments).
    pub fn disable_window_observer(mut self) -> Self {
        self.config.enable_window_observer = false;
        self
    }

    /// Build the final configuration.
    pub fn build(self) -> DesktopConfig {
        self.config
    }
}

impl Default for DesktopConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_reasonable_values() {
        let config = DesktopConfig::default();
        assert!(config.stage_width >= 200.0);
        assert!(config.stage_height >= 200.0);
        assert!(config.hit_threshold > 0);
        assert!(config.panel_opacity > 0.0 && config.panel_opacity <= 1.0);
        assert!(config.max_fps >= 0);
    }

    #[test]
    fn test_builder_overrides() {
        let config = DesktopConfigBuilder::new()
            .stage_size(800.0, 600.0)
            .max_fps(144)
            .hit_threshold(128)
            .build();

        assert_eq!(config.stage_width, 800.0);
        assert_eq!(config.stage_height, 600.0);
        assert_eq!(config.max_fps, 144);
        assert_eq!(config.hit_threshold, 128);
    }

    #[test]
    fn test_disable_input_observer() {
        let config = DesktopConfigBuilder::new()
            .disable_input_observer()
            .build();
        assert!(!config.enable_input_observer);
    }
}
