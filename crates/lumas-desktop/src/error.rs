//! # Desktop Error Hierarchy
//!
//! Complete, structured error types for the Lumas Desktop Engine.
//!
//! Every operation — window creation, hit testing, monitor enumeration,
//! input observation — returns `DesktopError` on failure.
//!
//! # Thread Safety
//!
//! All error types are `Send + Sync` via `thiserror` and `Box<dyn Error>`.

use std::fmt;

/// Identifiers used in error variants.
pub use crate::window::WindowId;
pub use crate::overlay::OverlayId;
pub use crate::monitor::MonitorId;
pub use crate::window::WindowState;

/// Primary error type for the Desktop Engine.
#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    /// The current platform is not supported.
    #[error("Platform not supported: {platform}")]
    UnsupportedPlatform {
        /// The platform identifier.
        platform: &'static str,
    },

    /// Window creation failed.
    #[error("Window creation failed: {reason}")]
    WindowCreationFailed {
        /// Description of the failure.
        reason: String,
    },

    /// Window not found in the registry.
    #[error("Window '{id}' not found")]
    WindowNotFound {
        /// The window ID.
        id: WindowId,
    },

    /// Overlay not found.
    #[error("Overlay '{id}' not found")]
    OverlayNotFound {
        /// The overlay ID.
        id: OverlayId,
    },

    /// Monitor not found or disconnected.
    #[error("Monitor '{id}' not found or disconnected")]
    MonitorNotFound {
        /// The monitor ID.
        id: MonitorId,
    },

    /// Invalid window state transition.
    #[error("Invalid window state transition: {from:?} → {to:?}")]
    InvalidWindowStateTransition {
        /// The source state.
        from: WindowState,
        /// The target state.
        to: WindowState,
    },

    /// Hit test alpha mask not initialized.
    #[error("Hit test alpha mask not initialized for window '{id}'")]
    HitTestMaskNotInitialized {
        /// The window ID.
        id: WindowId,
    },

    /// Shared memory error for alpha mask IPC.
    #[error("Alpha mask shared memory error: {reason}")]
    SharedMemoryError {
        /// Description of the error.
        reason: String,
    },

    /// DPI context unavailable for a monitor.
    #[error("DPI context unavailable for monitor '{id}'")]
    DpiContextUnavailable {
        /// The monitor ID.
        id: MonitorId,
    },

    /// Event loop has exited.
    #[error("Desktop command channel closed; event loop has exited")]
    EventLoopExited,

    /// Desktop command timed out.
    #[error("Desktop command timed out after {timeout_ms}ms: {command}")]
    CommandTimeout {
        /// The command name.
        command: &'static str,
        /// The timeout duration.
        timeout_ms: u64,
    },

    /// Global input hook failed to install.
    #[error("Global input hook failed to install: {reason}")]
    InputHookFailed {
        /// Description of the failure.
        reason: String,
    },

    /// Drag-drop registration failed.
    #[error("Drag-drop registration failed for window '{id}': {reason}")]
    DragDropFailed {
        /// The window ID.
        id: WindowId,
        /// Description of the failure.
        reason: String,
    },

    /// Z-order operation failed.
    #[error("Z-order operation failed: {reason}")]
    ZOrderFailed {
        /// Description of the failure.
        reason: String,
    },

    /// Platform-specific operation failed.
    #[error("Platform operation failed: {operation}: {source}")]
    PlatformError {
        /// The operation name.
        operation: &'static str,
        /// The underlying error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Permission denied for an operation.
    #[error("Permission denied: {operation} requires {permission}")]
    PermissionDenied {
        /// The operation that was denied.
        operation: &'static str,
        /// The required permission.
        permission: &'static str,
    },
}

impl DesktopError {
    /// Returns `true` if the supervisor can automatically recover from this error.
    pub fn is_recoverable(&self) -> bool {
        match self {
            DesktopError::UnsupportedPlatform { .. } => false,
            DesktopError::WindowCreationFailed { .. } => false,
            DesktopError::WindowNotFound { .. } => false,
            DesktopError::OverlayNotFound { .. } => false,
            DesktopError::MonitorNotFound { .. } => false,
            DesktopError::InvalidWindowStateTransition { .. } => false,
            DesktopError::HitTestMaskNotInitialized { .. } => true,
            DesktopError::SharedMemoryError { .. } => true,
            DesktopError::DpiContextUnavailable { .. } => true,
            DesktopError::EventLoopExited => false,
            DesktopError::CommandTimeout { .. } => true,
            DesktopError::InputHookFailed { .. } => true,
            DesktopError::DragDropFailed { .. } => true,
            DesktopError::ZOrderFailed { .. } => true,
            DesktopError::PlatformError { .. } => true,
            DesktopError::PermissionDenied { .. } => false,
        }
    }

    /// Returns a human-readable suggested action for the operator.
    pub fn suggested_action(&self) -> &'static str {
        match self {
            DesktopError::UnsupportedPlatform { .. } => {
                "Lumas requires Windows 11, macOS 13+, or Linux (X11/Wayland)."
            }
            DesktopError::WindowCreationFailed { .. } => {
                "Check display server availability and graphics drivers."
            }
            DesktopError::WindowNotFound { .. } => {
                "Ensure the window was created before accessing it."
            }
            DesktopError::OverlayNotFound { .. } => {
                "Ensure the overlay was created before accessing it."
            }
            DesktopError::MonitorNotFound { .. } => {
                "The monitor may have been disconnected. Re-check available monitors."
            }
            DesktopError::InvalidWindowStateTransition { .. } => {
                "This indicates a bug in the window state machine. Report with logs."
            }
            DesktopError::HitTestMaskNotInitialized { .. } => {
                "Ensure the render process has sent the first alpha mask."
            }
            DesktopError::SharedMemoryError { .. } => {
                "Check shared memory configuration and permissions."
            }
            DesktopError::DpiContextUnavailable { .. } => {
                "The monitor may have been disconnected. The primary monitor DPI will be used."
            }
            DesktopError::EventLoopExited => {
                "The desktop event loop has stopped. Restart the application."
            }
            DesktopError::CommandTimeout { .. } => {
                "The event loop may be blocked. Check for deadlocks or long operations."
            }
            DesktopError::InputHookFailed { .. } => {
                "Check system permissions for accessibility/input monitoring."
            }
            DesktopError::DragDropFailed { .. } => {
                "Ensure the window is valid and the OS supports drag-drop."
            }
            DesktopError::ZOrderFailed { .. } => {
                "The compositor may not support the requested z-order operation."
            }
            DesktopError::PlatformError { .. } => {
                "Check OS-level status and permissions."
            }
            DesktopError::PermissionDenied { .. } => {
                "Grant the required permission in System Settings."
            }
        }
    }
}

impl From<std::io::Error> for DesktopError {
    fn from(err: std::io::Error) -> Self {
        DesktopError::PlatformError {
            operation: "io_operation",
            source: Box::new(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_error_display() {
        let err = DesktopError::UnsupportedPlatform {
            platform: "test_os",
        };
        let msg = err.to_string();
        assert!(msg.contains("test_os"));
    }

    #[test]
    fn test_windows_not_found_is_not_recoverable() {
        let err = DesktopError::WindowNotFound {
            id: WindowId::new(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_input_hook_failed_is_recoverable() {
        let err = DesktopError::InputHookFailed {
            reason: "permission denied".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_suggested_action_not_empty() {
        let err = DesktopError::EventLoopExited;
        let action = err.suggested_action();
        assert!(!action.is_empty());
    }
}
