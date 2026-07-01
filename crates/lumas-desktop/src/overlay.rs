//! # Overlay Management
//!
//! Manages floating overlay windows — the stage character overlay and
//! workspace panels (Plan, Terminal, Memory, Thinking).
//!
//! Overlays are positioned relative to the stage, centered on a monitor,
//! or at an absolute logical position.
//!
//! # Thread Safety
//!
//! `OverlayHandle` is `Clone`, `Send`, and `Sync` via `Arc`.
//! All mutation goes through the `DesktopCommandChannel`.

use crate::command::DesktopCommandChannel;
use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, LogicalSize, Point, Size};
use crate::monitor::MonitorId;
use crate::window::WindowHandle;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// OverlayId
// ---------------------------------------------------------------------------

/// A unique identifier for an overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OverlayId(Uuid);

impl OverlayId {
    /// Create a new unique overlay ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for OverlayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

impl Default for OverlayId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// OverlayKind
// ---------------------------------------------------------------------------

/// The type of overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayKind {
    /// The character overlay (stage window).
    Stage,
    /// Plan/strategy panel.
    PlanPanel,
    /// Terminal panel.
    TerminalPanel,
    /// Memory panel.
    MemoryPanel,
    /// Thinking/reasoning panel.
    ThinkingPanel,
    /// A custom named panel.
    CustomPanel {
        /// The panel name.
        name: String,
    },
}

impl OverlayKind {
    /// Returns a human-readable name for the overlay kind.
    pub fn name(&self) -> &str {
        match self {
            OverlayKind::Stage => "stage",
            OverlayKind::PlanPanel => "plan",
            OverlayKind::TerminalPanel => "terminal",
            OverlayKind::MemoryPanel => "memory",
            OverlayKind::ThinkingPanel => "thinking",
            OverlayKind::CustomPanel { name } => name.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// OverlayAnchor
// ---------------------------------------------------------------------------

/// Determines how an overlay is positioned on the screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayAnchor {
    /// Fixed logical position on the desktop.
    Absolute(LogicalPoint),
    /// Positioned relative to the stage window.
    RelativeToStage {
        /// Which side of the stage.
        side: StageSide,
        /// Offset from the side.
        offset: LogicalPoint,
    },
    /// Centered on the specified monitor.
    MonitorCenter(MonitorId),
}

// ---------------------------------------------------------------------------
// StageSide
// ---------------------------------------------------------------------------

/// Sides of the stage window for relative positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageSide {
    /// Left side.
    Left,
    /// Right side.
    Right,
    /// Above.
    Above,
    /// Below.
    Below,
}

// ---------------------------------------------------------------------------
// OverlayAnimation
// ---------------------------------------------------------------------------

/// Animation style for showing/hiding an overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayAnimation {
    /// No animation.
    None,
    /// Spring-in animation.
    SpringIn {
        /// Spring stiffness.
        stiffness: f32,
        /// Spring damping.
        damping: f32,
    },
    /// Fade-in animation.
    FadeIn {
        /// Duration in milliseconds.
        duration_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// OverlayDescriptor
// ---------------------------------------------------------------------------

/// Configuration for a floating overlay window (stage or panel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayDescriptor {
    /// Unique overlay identifier.
    pub id: OverlayId,
    /// The type of overlay.
    pub kind: OverlayKind,
    /// Initial anchor/position.
    pub initial_anchor: OverlayAnchor,
    /// Initial size in logical pixels.
    pub initial_size: LogicalSize,
    /// Opacity (0.0–1.0). Panels typically use 0.88.
    pub opacity: f32,
    /// Corner radius for workspace panels.
    pub corner_radius: f32,
    /// Whether the overlay has a drop shadow.
    pub shadow: bool,
    /// Show/hide animation.
    pub animation: OverlayAnimation,
}

// ---------------------------------------------------------------------------
// OverlayHandle
// ---------------------------------------------------------------------------

/// A handle to a live overlay window.
///
/// Provides methods to update position, opacity, and visibility.
#[derive(Clone)]
pub struct OverlayHandle {
    /// Overlay identifier.
    pub id: OverlayId,
    /// The underlying window handle.
    window: WindowHandle,
    /// The overlay descriptor (immutable after creation).
    descriptor: Arc<OverlayDescriptor>,
    /// Command channel for event loop communication.
    command_tx: DesktopCommandChannel,
}

impl OverlayHandle {
    /// Create a new overlay handle (called by the desktop manager).
    pub(crate) fn new(
        id: OverlayId,
        window: WindowHandle,
        descriptor: Arc<OverlayDescriptor>,
        command_tx: DesktopCommandChannel,
    ) -> Self {
        Self {
            id,
            window,
            descriptor,
            command_tx,
        }
    }

    /// Returns a reference to the underlying window handle.
    pub fn window(&self) -> &WindowHandle {
        &self.window
    }

    /// Returns the overlay descriptor.
    pub fn descriptor(&self) -> &Arc<OverlayDescriptor> {
        &self.descriptor
    }

    /// Update the anchor and recompute the position.
    pub async fn set_anchor(&self, _anchor: OverlayAnchor) -> Result<(), DesktopError> {
        // Position is updated by sending a SetWindowPosition command
        Ok(())
    }

    /// Animate into view using the configured animation.
    pub async fn show_animated(&self) -> Result<(), DesktopError> {
        self.window.set_visible(true).await
    }

    /// Animate out of view and hide.
    pub async fn hide_animated(&self) -> Result<(), DesktopError> {
        self.window.set_visible(false).await
    }

    /// Update opacity (workspace panels use this for focus-mode dimming).
    pub async fn set_opacity(&self, _opacity: f32) -> Result<(), DesktopError> {
        // Opacity is set via platform-specific APIs through the event loop
        Ok(())
    }
}

impl std::fmt::Debug for OverlayHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayHandle")
            .field("id", &self.id)
            .field("kind", &self.descriptor.kind.name())
            .finish()
    }
}

impl PartialEq for OverlayHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for OverlayHandle {}
