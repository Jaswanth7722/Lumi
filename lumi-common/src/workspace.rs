//! # Workspace System — Panel Types and Positioning (Chapter 12)
//!
//! Defines the floating workspace panel types, lifecycle states,
//! and relative positioning logic.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Panel Types
// ---------------------------------------------------------------------------

/// Types of workspace panels displayed beside Lumi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelType {
    /// Active task plan with step statuses.
    Plan,
    /// Live terminal output during command execution.
    Terminal,
    /// Inference thinking indicator.
    Thinking,
    /// Retrieved memory display.
    Memory,
    /// Custom panel for plugins or system use.
    Custom(String),
}

/// Lifecycle state of a workspace panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelState {
    /// Panel is not visible.
    Hidden,
    /// Panel is animating into view (spring scale-in).
    Appearing,
    /// Panel is fully visible.
    Visible,
    /// Panel content is being updated.
    Updating,
    /// Panel is animating out of view.
    Dismissing,
    /// User has pinned the panel (no auto-dismiss).
    Pinned,
}

/// A workspace command emitted by the AI Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkspaceCommand {
    /// Show a panel with initial content.
    Show {
        panel_type: PanelType,
        content: serde_json::Value,
    },
    /// Update the content of an existing panel.
    Update {
        panel_id: String,
        content: serde_json::Value,
    },
    /// Hide and dismiss a panel.
    Hide { panel_id: String },
    /// Pin a panel to prevent auto-dismiss.
    Pin { panel_id: String },
    /// Unpin a panel (restores auto-dismiss behavior).
    Unpin { panel_id: String },
}

/// Unique identifier for a workspace panel instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PanelId(pub String);

impl PanelId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for PanelId {
    fn default() -> Self {
        Self::new()
    }
}

/// Content to display in a workspace panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelContent {
    pub title: Option<String>,
    pub sections: Vec<PanelSection>,
    pub status: Option<PanelStatus>,
    pub metadata: Option<serde_json::Value>,
}

/// A section within a panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelSection {
    pub heading: Option<String>,
    pub body: String,
    pub content_type: SectionContentType,
}

/// Type of content in a panel section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionContentType {
    Text,
    Code,
    TerminalOutput,
    StepList,
    MemoryList,
    Table,
}

/// Status indicator for a panel or step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelStatus {
    Idle,
    Running { progress: f32 },
    Completed { duration_ms: u64 },
    Failed { error: String },
    Pending,
}

// ---------------------------------------------------------------------------
// Panel Positioning
// ---------------------------------------------------------------------------

/// Preferred side for panel positioning relative to Lumi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferredSide {
    Right,
    Left,
    Above,
}

/// A 2D rectangle for positioning calculations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn left(&self) -> f32 {
        self.x
    }

    pub fn top(&self) -> f32 {
        self.y
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    pub fn center(&self) -> (f32, f32) {
        (self.center_x(), self.center_y())
    }

    pub fn contains(&self, point: (f32, f32)) -> bool {
        point.0 >= self.x
            && point.0 <= self.right()
            && point.1 >= self.y
            && point.1 <= self.bottom()
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        self.x <= other.x
            && self.right() >= other.right()
            && self.y <= other.y
            && self.bottom() >= other.bottom()
    }
}

/// Monitor information for positioning calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub work_area: Rect,
    pub scale_factor: f32,
    pub is_primary: bool,
}

/// Size of a panel in screen pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// Compute the best panel position relative to Lumi's bounds,
/// trying each preferred side and falling back to the center of the monitor.
pub fn compute_panel_position(
    lumi_bounds: Rect,
    panel_size: Size,
    monitor: MonitorInfo,
    preferred_side: PreferredSide,
) -> (f32, f32) {
    let candidates = match preferred_side {
        PreferredSide::Right => vec![
            (
                (lumi_bounds.right() + 16.0, lumi_bounds.top()),
                PreferredSide::Right,
            ),
            (
                (
                    lumi_bounds.left() - panel_size.width - 16.0,
                    lumi_bounds.top(),
                ),
                PreferredSide::Left,
            ),
            (
                (
                    lumi_bounds.center_x() - panel_size.width / 2.0,
                    lumi_bounds.top() - panel_size.height - 16.0,
                ),
                PreferredSide::Above,
            ),
        ],
        PreferredSide::Left => vec![
            (
                (
                    lumi_bounds.left() - panel_size.width - 16.0,
                    lumi_bounds.top(),
                ),
                PreferredSide::Left,
            ),
            (
                (lumi_bounds.right() + 16.0, lumi_bounds.top()),
                PreferredSide::Right,
            ),
            (
                (
                    lumi_bounds.center_x() - panel_size.width / 2.0,
                    lumi_bounds.top() - panel_size.height - 16.0,
                ),
                PreferredSide::Above,
            ),
        ],
        PreferredSide::Above => vec![
            (
                (
                    lumi_bounds.center_x() - panel_size.width / 2.0,
                    lumi_bounds.top() - panel_size.height - 16.0,
                ),
                PreferredSide::Above,
            ),
            (
                (lumi_bounds.right() + 16.0, lumi_bounds.top()),
                PreferredSide::Right,
            ),
            (
                (
                    lumi_bounds.left() - panel_size.width - 16.0,
                    lumi_bounds.top(),
                ),
                PreferredSide::Left,
            ),
        ],
    };

    for ((x, y), _side) in &candidates {
        let candidate = Rect::new(*x, *y, panel_size.width, panel_size.height);
        if monitor.work_area.contains_rect(&candidate) {
            return (*x, *y);
        }
    }

    // Fallback: center on monitor work area
    let cx = monitor.work_area.center_x() - panel_size.width / 2.0;
    let cy = monitor.work_area.center_y() - panel_size.height / 2.0;
    (cx, cy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(rect.contains((50.0, 50.0)));
        assert!(!rect.contains((150.0, 50.0)));
        assert!(!rect.contains((-1.0, 50.0)));
    }

    #[test]
    fn test_panel_positioning_fallback() {
        let lumi = Rect::new(500.0, 500.0, 100.0, 150.0);
        let panel_size = Size {
            width: 300.0,
            height: 200.0,
        };
        let monitor = MonitorInfo {
            work_area: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            scale_factor: 1.0,
            is_primary: true,
        };

        // Right side should fit
        let (x, y) = compute_panel_position(lumi, panel_size, monitor, PreferredSide::Right);
        assert!(x > lumi.right());
        assert!(y >= lumi.top());
    }

    #[test]
    fn test_panel_positioning_edge() {
        let lumi = Rect::new(1800.0, 500.0, 100.0, 150.0);
        let panel_size = Size {
            width: 300.0,
            height: 200.0,
        };
        let monitor = MonitorInfo {
            work_area: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            scale_factor: 1.0,
            is_primary: true,
        };

        // Right side won't fit (1920 - 1800 < 300 + 16), should fall back to left
        let (x, y) = compute_panel_position(lumi, panel_size, monitor, PreferredSide::Right);
        // Should be on the left side of Lumi
        assert!(x < lumi.left());
    }
}
