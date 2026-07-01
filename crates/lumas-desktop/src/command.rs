//! # Desktop Command Channel
//!
//! Cross-thread communication bridge to the winit event loop.
//!
//! The winit `EventLoop` is `!Send` and must run on a dedicated thread.
//! All cross-thread requests to the event loop use typed commands over
//! a bounded crossbeam channel with `tokio::sync::oneshot` responders.
//!
//! # Thread Safety
//!
//! `DesktopCommandChannel` is `Clone`, `Send`, and `Sync`. It is the
//! only way to communicate with the event loop thread from async tasks.

use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, LogicalSize};
use crate::monitor::MonitorInfo;
use crate::window::{WindowDescriptor, WindowId};
use crossbeam_channel::Sender;
use std::time::Duration;
use tokio::sync::oneshot;

/// Commands sent from async tasks to the event loop thread.
///
/// Each command carries a `oneshot::Sender` for the result, allowing
/// the caller to await the response.
#[derive(Debug)]
pub enum DesktopCommand {
    /// Create a new window.
    CreateWindow {
        /// Window descriptor.
        descriptor: WindowDescriptor,
        /// Responder for the created handle.
        responder: oneshot::Sender<Result<super::window::WindowHandle, DesktopError>>,
    },
    /// Destroy a window.
    DestroyWindow {
        /// Window ID.
        id: WindowId,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Set window position.
    SetWindowPosition {
        /// Window ID.
        id: WindowId,
        /// New position in logical pixels.
        position: LogicalPoint,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Set window size.
    SetWindowSize {
        /// Window ID.
        id: WindowId,
        /// New size in logical pixels.
        size: LogicalSize,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Show or hide a window.
    SetWindowVisible {
        /// Window ID.
        id: WindowId,
        /// Whether the window should be visible.
        visible: bool,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Enable or disable click-through.
    SetClickThrough {
        /// Window ID.
        id: WindowId,
        /// Whether click-through is enabled.
        enabled: bool,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Bring a window to the front.
    BringToFront {
        /// Window ID.
        id: WindowId,
        /// Responder.
        responder: oneshot::Sender<Result<(), DesktopError>>,
    },
    /// Refresh the monitor list.
    RefreshMonitors {
        /// Responder with updated monitor list.
        responder: oneshot::Sender<Result<Vec<MonitorInfo>, DesktopError>>,
    },
    /// Shutdown the event loop.
    Shutdown,
}

// ---------------------------------------------------------------------------
// DesktopCommandChannel
// ---------------------------------------------------------------------------

/// The sending end of the command channel.
///
/// `Clone`, `Send`, `Sync`. All `WindowHandle` and `OverlayHandle` methods
/// send commands through this channel.
#[derive(Clone)]
pub struct DesktopCommandChannel {
    /// The underlying crossbeam channel sender.
    inner: Sender<DesktopCommand>,
}

impl DesktopCommandChannel {
    /// Create a new command channel wrapping a crossbeam sender.
    pub fn new(inner: Sender<DesktopCommand>) -> Self {
        Self { inner }
    }

    /// Send a command and await the response with a configurable timeout.
    ///
    /// # Type Parameters
    ///
    /// * `T` — The response type.
    ///
    /// # Arguments
    ///
    /// * `build` — A closure that creates the `DesktopCommand` from a responder.
    /// * `timeout_ms` — Maximum time to wait for a response.
    /// * `command_name` — Name of the command (for error messages).
    ///
    /// # Errors
    ///
    /// Returns `DesktopError::EventLoopExited` if the channel is closed.
    /// Returns `DesktopError::CommandTimeout` if the response times out.
    pub async fn send<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, DesktopError>>) -> DesktopCommand,
        timeout_ms: u64,
        command_name: &'static str,
    ) -> Result<T, DesktopError> {
        let (tx, rx) = oneshot::channel();
        let command = build(tx);

        self.inner.send(command).map_err(|_| DesktopError::EventLoopExited)?;

        tokio::time::timeout(Duration::from_millis(timeout_ms), rx)
            .await
            .map_err(|_| DesktopError::CommandTimeout {
                command: command_name,
                timeout_ms,
            })?
            .map_err(|_| DesktopError::EventLoopExited)?
    }
}

impl DesktopCommandChannel {
    /// Send a command without awaiting a response (fire-and-forget).
    ///
    /// Used for commands like `Shutdown` that don't need a response.
    ///
    /// # Errors
    /// Returns `DesktopError::EventLoopExited` if the channel is closed.
    pub fn send_raw(&self, command: DesktopCommand) -> Result<(), DesktopError> {
        self.inner.send(command).map_err(|_| DesktopError::EventLoopExited)
    }
}

impl std::fmt::Debug for DesktopCommandChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopCommandChannel").finish()
    }
}
