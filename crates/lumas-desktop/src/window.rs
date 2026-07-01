//! # Window Management
//!
//! Window descriptor, state machine, and handle for all Lumi-owned windows.
//!
//! Every window — stage, workspace panel, settings, notification — is created
//! through this module. The `WindowHandle` provides a thread-safe reference
//! to a window that can be used from any async context.
//!
//! # Thread Safety
//!
//! `WindowHandle` is `Clone`, `Send`, and `Sync` via `Arc`.
//! All mutation goes through `DesktopCommandChannel` to the event loop thread.

use crate::command::{DesktopCommand, DesktopCommandChannel};
use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, LogicalSize, Point, Size};
use crate::metrics::DesktopMetrics;
use chrono::{DateTime, Utc};
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WindowId
// ---------------------------------------------------------------------------

/// A unique identifier for a Lumi-owned window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(Uuid);

impl WindowId {
    /// Create a new unique window ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0.to_string()[..8])
    }
}

impl Default for WindowId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// WindowKind
// ---------------------------------------------------------------------------

/// The type/kind of a Lumas window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowKind {
    /// The character overlay — transparent, always-on-top.
    Stage,
    /// Task panels — translucent, floating, dismissable.
    WorkspacePanel,
    /// Standard bordered settings window.
    Settings,
    /// Transient notification (auto-dismiss).
    Notification,
}

impl WindowKind {
    /// Returns a human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            WindowKind::Stage => "stage",
            WindowKind::WorkspacePanel => "workspace_panel",
            WindowKind::Settings => "settings",
            WindowKind::Notification => "notification",
        }
    }
}

// ---------------------------------------------------------------------------
// WindowState
// ---------------------------------------------------------------------------

/// Lifecycle state of a Lumas window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowState {
    /// Window is being created.
    Creating,
    /// Window is visible on screen.
    Visible,
    /// Window is hidden but not destroyed.
    Hidden,
    /// Window is minimized (OS-level).
    Minimized,
    /// Window has keyboard focus.
    Focused,
    /// Window has lost keyboard focus.
    Unfocused,
    /// Window is closing (in destruction process).
    Closing,
    /// Window is closed (terminal state).
    Closed,
}

impl WindowState {
    /// Returns valid target states from this state.
    pub fn valid_transitions(&self) -> &'static [WindowState] {
        match self {
            WindowState::Creating => &[WindowState::Visible, WindowState::Closed],
            WindowState::Visible => &[
                WindowState::Hidden,
                WindowState::Minimized,
                WindowState::Focused,
                WindowState::Unfocused,
                WindowState::Closing,
            ],
            WindowState::Hidden => &[WindowState::Visible, WindowState::Closing],
            WindowState::Minimized => &[WindowState::Visible, WindowState::Closing],
            WindowState::Focused => &[
                WindowState::Unfocused,
                WindowState::Hidden,
                WindowState::Minimized,
                WindowState::Closing,
            ],
            WindowState::Unfocused => &[
                WindowState::Focused,
                WindowState::Hidden,
                WindowState::Minimized,
                WindowState::Closing,
            ],
            WindowState::Closing => &[WindowState::Closed],
            WindowState::Closed => &[], // Terminal
        }
    }

    /// Returns `true` if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, WindowState::Closed)
    }

    /// Returns `true` if the window is visible on screen.
    pub fn is_visible(&self) -> bool {
        matches!(
            self,
            WindowState::Visible | WindowState::Focused | WindowState::Unfocused
        )
    }
}

impl std::fmt::Display for WindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowState::Creating => write!(f, "Creating"),
            WindowState::Visible => write!(f, "Visible"),
            WindowState::Hidden => write!(f, "Hidden"),
            WindowState::Minimized => write!(f, "Minimized"),
            WindowState::Focused => write!(f, "Focused"),
            WindowState::Unfocused => write!(f, "Unfocused"),
            WindowState::Closing => write!(f, "Closing"),
            WindowState::Closed => write!(f, "Closed"),
        }
    }
}

// ---------------------------------------------------------------------------
// WindowStateMachine
// ---------------------------------------------------------------------------

/// State machine enforcing valid window state transitions.
pub struct WindowStateMachine {
    /// Current state.
    current: WindowState,
    /// Transition history (most recent first, max 20).
    history: VecDeque<(WindowState, DateTime<Utc>)>,
}

impl WindowStateMachine {
    /// Create a new window state machine starting at `Creating`.
    pub fn new() -> Self {
        Self {
            current: WindowState::Creating,
            history: VecDeque::with_capacity(20),
        }
    }

    /// Transition to a new state.
    ///
    /// # Errors
    ///
    /// Returns `DesktopError::InvalidWindowStateTransition` if the transition
    /// is not allowed.
    pub fn transition(&mut self, to: WindowState) -> Result<(), DesktopError> {
        let from = self.current;

        if from.is_terminal() {
            return Err(DesktopError::InvalidWindowStateTransition { from, to });
        }

        let allowed = from.valid_transitions();
        if !allowed.contains(&to) {
            return Err(DesktopError::InvalidWindowStateTransition { from, to });
        }

        self.current = to;
        self.history.push_front((from, Utc::now()));
        if self.history.len() > 20 {
            self.history.pop_back();
        }

        Ok(())
    }

    /// Returns the current state.
    pub fn current(&self) -> WindowState {
        self.current
    }

    /// Returns the transition history.
    pub fn history(&self) -> &VecDeque<(WindowState, DateTime<Utc>)> {
        &self.history
    }
}

impl Default for WindowStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// WindowDescriptor
// ---------------------------------------------------------------------------

/// Static description of a window to be created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDescriptor {
    /// Unique window identifier.
    pub id: WindowId,
    /// The type/kind of window.
    pub kind: WindowKind,
    /// Window title.
    pub title: String,
    /// Initial position in logical pixels.
    pub initial_position: LogicalPoint,
    /// Initial size in logical pixels.
    pub initial_size: LogicalSize,
    /// Minimum size in logical pixels.
    pub min_size: Option<LogicalSize>,
    /// Maximum size in logical pixels.
    pub max_size: Option<LogicalSize>,
    /// Whether to show window decorations (title bar, borders).
    pub decorations: bool,
    /// Whether the window is transparent (for compositing).
    pub transparent: bool,
    /// Whether the window should be always-on-top.
    pub always_on_top: bool,
    /// Whether mouse events pass through the window initially.
    pub click_through: bool,
    /// Whether the window is resizable.
    pub resizable: bool,
}

// ---------------------------------------------------------------------------
// WindowHandleInner
// ---------------------------------------------------------------------------

struct WindowHandleInner {
    /// Window descriptor (immutable after creation).
    descriptor: Arc<WindowDescriptor>,
    /// Window state machine.
    state: RwLock<WindowStateMachine>,
    /// The winit window ID.
    winit_id: winit::window::WindowId,
    /// Raw window handle for GPU surface creation (rwh 0.6 is Send+Sync).
    raw_handle: Option<RawWindowHandle>,
    /// Channel for sending commands to the event loop thread.
    command_tx: DesktopCommandChannel,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

// ---------------------------------------------------------------------------
// WindowHandle
// ---------------------------------------------------------------------------

/// A live handle to a created window.
///
/// Cloning a handle is O(1). Operations are async and go through the
/// `DesktopCommandChannel` to the event loop thread.
#[derive(Clone)]
pub struct WindowHandle {
    /// The window ID.
    pub id: WindowId,
    inner: Arc<WindowHandleInner>,
}

impl WindowHandle {
    /// Create a new window handle (called by the event loop thread).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: WindowId,
        descriptor: Arc<WindowDescriptor>,
        winit_id: winit::window::WindowId,
        raw_handle: Option<RawWindowHandle>,
        command_tx: DesktopCommandChannel,
        metrics: Arc<DesktopMetrics>,
    ) -> Self {
        Self {
            id,
            inner: Arc::new(WindowHandleInner {
                descriptor,
                state: RwLock::new(WindowStateMachine::new()),
                winit_id,
                raw_handle,
                command_tx,
                metrics,
            }),
        }
    }

    /// Returns the current window state.
    pub fn state(&self) -> WindowState {
        self.inner.state.read().current()
    }

    /// Transition the window state.
    pub(crate) fn transition_state(&self, to: WindowState) -> Result<(), DesktopError> {
        self.inner.state.write().transition(to)
    }

    /// Returns the window descriptor (immutable metadata).
    pub fn descriptor(&self) -> Arc<WindowDescriptor> {
        self.inner.descriptor.clone()
    }

    /// Returns the winit window ID.
    pub fn winit_id(&self) -> winit::window::WindowId {
        self.inner.winit_id
    }

    /// Get the raw window handle for GPU surface creation.
    ///
    /// The handle is captured at window creation time from the winit window and
    /// is valid for the lifetime of the window. `RawWindowHandle` is `Send + Sync`
    /// in rwh 0.6, so it can be safely accessed from any thread.
    pub fn raw_window_handle(&self) -> Option<RawWindowHandle> {
        self.inner.raw_handle
    }

    /// Get the raw display handle.
    pub fn raw_display_handle(&self) -> Option<RawDisplayHandle> {
        // Display handle is platform-specific and typically not needed
        // for wgpu surface creation when a window handle is provided.
        None
    }

    /// Move the window to a new logical position.
    pub async fn set_position(&self, pos: LogicalPoint) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::SetWindowPosition {
                id: self.id,
                position: pos,
                responder,
            },
            5000,
            "set_position",
        ).await
    }

    /// Resize the window to a new logical size.
    pub async fn set_size(&self, size: LogicalSize) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::SetWindowSize {
                id: self.id,
                size,
                responder,
            },
            5000,
            "set_size",
        ).await
    }

    /// Show or hide the window.
    pub async fn set_visible(&self, visible: bool) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::SetWindowVisible {
                id: self.id,
                visible,
                responder,
            },
            5000,
            "set_visible",
        ).await
    }

    /// Enable or disable mouse event pass-through.
    pub async fn set_click_through(&self, enabled: bool) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::SetClickThrough {
                id: self.id,
                enabled,
                responder,
            },
            5000,
            "set_click_through",
        ).await
    }

    /// Bring this window to the front of all Lumi-owned windows.
    pub async fn bring_to_front(&self) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::BringToFront {
                id: self.id,
                responder,
            },
            5000,
            "bring_to_front",
        ).await
    }

    /// Destroy this window.
    pub async fn destroy(&self) -> Result<(), DesktopError> {
        self.inner.command_tx.send(
            |responder| DesktopCommand::DestroyWindow {
                id: self.id,
                responder,
            },
            5000,
            "destroy_window",
        ).await
    }
}

impl std::fmt::Debug for WindowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowHandle")
            .field("id", &self.id)
            .field("state", &self.state())
            .finish()
    }
}

impl PartialEq for WindowHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for WindowHandle {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_transitions() {
        let mut sm = WindowStateMachine::new();
        assert_eq!(sm.current(), WindowState::Creating);

        sm.transition(WindowState::Visible).unwrap();
        assert_eq!(sm.current(), WindowState::Visible);

        sm.transition(WindowState::Focused).unwrap();
        assert_eq!(sm.current(), WindowState::Focused);

        sm.transition(WindowState::Unfocused).unwrap();
        assert_eq!(sm.current(), WindowState::Unfocused);
    }

    #[test]
    fn test_invalid_transition() {
        let mut sm = WindowStateMachine::new();
        // Can't go from Creating to Focused directly
        let result = sm.transition(WindowState::Focused);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_state_rejects_transitions() {
        let mut sm = WindowStateMachine::new();
        sm.transition(WindowState::Visible).unwrap();
        sm.transition(WindowState::Closing).unwrap();
        sm.transition(WindowState::Closed).unwrap();
        assert!(sm.current().is_terminal());
        assert!(sm.transition(WindowState::Visible).is_err());
    }

    #[test]
    fn test_window_kind_names() {
        assert_eq!(WindowKind::Stage.name(), "stage");
        assert_eq!(WindowKind::Settings.name(), "settings");
    }

    #[test]
    fn test_window_descriptor_creation() {
        let desc = WindowDescriptor {
            id: WindowId::new(),
            kind: WindowKind::Stage,
            title: "Lumas Stage".into(),
            initial_position: Point::new(100.0, 100.0),
            initial_size: Size::new(400.0, 600.0),
            min_size: None,
            max_size: None,
            decorations: false,
            transparent: true,
            always_on_top: true,
            click_through: true,
            resizable: false,
        };
        assert_eq!(desc.kind, WindowKind::Stage);
        assert!(desc.transparent);
    }
}
