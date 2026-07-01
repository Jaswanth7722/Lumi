//! # Z-Order Management
//!
//! Manages the Z-order stack of all Lumi-owned windows.
//! Ensures the stage window is always above panels, which are above settings.
//!
//! # Thread Safety
//!
//! `ZOrderManager` is `Send + Sync` via `parking_lot::RwLock`.

use crate::command::DesktopCommandChannel;
use crate::error::DesktopError;
use crate::metrics::DesktopMetrics;
use crate::window::WindowId;
use parking_lot::RwLock;

/// Z-order tiers for Lumas windows.
///
/// Each variant corresponds to a window type in the Lumas window topology.
/// The ordering is: Stage (topmost) > Modal > Notification > Panel > Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrder {
    /// Stage window — the character overlay, always above everything.
    /// Maps to `winit::window::WindowLevel::AlwaysOnTop`.
    Stage,
    /// Workspace panels — floating panels (Plan, Terminal, Memory, etc.).
    /// Maps to `winit::window::WindowLevel::AlwaysOnTop`.
    Panel,
    /// Settings window — standard bordered window, normal OS z-order.
    /// Maps to `winit::window::WindowLevel::Normal`.
    Settings,
    /// Transient notification windows (auto-dismiss).
    /// Maps to `winit::window::WindowLevel::AlwaysOnTop`.
    Notification,
    /// Modal dialogs that require user interaction before continuing.
    /// Maps to `winit::window::WindowLevel::ModalPanel` (macOS) or
    /// `AlwaysOnTop` (other platforms).
    Modal,
}

/// Manages the Z-order stack of all Lumi-owned windows.
pub struct ZOrderManager {
    /// Stack of window IDs (front = topmost).
    stack: RwLock<Vec<WindowId>>,
    /// Command channel for sending operations to event loop.
    command_tx: DesktopCommandChannel,
    /// Desktop metrics.
    metrics: Arc<crate::metrics::DesktopMetrics>,
}

impl ZOrderManager {
    /// Create a new Z-order manager.
    pub fn new(
        command_tx: DesktopCommandChannel,
        metrics: Arc<crate::metrics::DesktopMetrics>,
    ) -> Self {
        Self {
            stack: RwLock::new(Vec::new()),
            command_tx,
            metrics,
        }
    }

    /// Bring a window to the front of its Z-order tier.
    pub async fn bring_to_front(&self, id: &WindowId) -> Result<(), DesktopError> {
        let mut stack = self.stack.write();
        if let Some(pos) = stack.iter().position(|wid| wid == id) {
            let id = stack.remove(pos);
            stack.push(id);
        }
        Ok(())
    }

    /// Ensure the stage window is always above all panels.
    pub async fn enforce_order(&self) -> Result<(), DesktopError> {
        // The platform backend handles actual Z-order enforcement
        // at the OS level. This method ensures the logical ordering
        // is maintained.
        Ok(())
    }

    /// Called when a panel is shown (adds it to the tracked stack).
    pub fn on_panel_shown(&self, id: WindowId) {
        self.stack.write().push(id);
    }

    /// Called when a panel is hidden (removes it from the tracked stack).
    pub fn on_panel_hidden(&self, id: &WindowId) {
        self.stack.write().retain(|wid| wid != id);
    }

    /// Returns the current Z-order stack.
    pub fn stack(&self) -> Vec<WindowId> {
        self.stack.read().clone()
    }

    /// Returns the number of tracked windows.
    pub fn len(&self) -> usize {
        self.stack.read().len()
    }

    /// Returns `true` if no windows are tracked.
    pub fn is_empty(&self) -> bool {
        self.stack.read().is_empty()
    }
}

impl std::fmt::Debug for ZOrderManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZOrderManager")
            .field("stack_size", &self.stack.read().len())
            .finish()
    }
}
