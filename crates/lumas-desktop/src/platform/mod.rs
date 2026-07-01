//! Platform abstraction layer for the Desktop Engine.
//!
//! Each supported operating system provides a concrete implementation of the
//! `PlatformBackend` trait. The `create_backend()` factory selects the correct
//! backend at runtime based on the current platform.
//!
//! # Thread Safety
//! All platform backends must implement `Send + Sync`. The backend is created
//! once during `DesktopManager` initialization and shared across threads.
//!
//! # Platform Notes
//! - **macOS**: Uses `objc2` bindings for NSWindow and `core-graphics` for
//!   CGDisplay/CGEventTap APIs.
//! - **Windows**: Uses the `windows` crate for HWND, `SetLayeredWindowAttributes`,
//!   `WinEventHook`, and `SetWindowsHookEx`.
//! - **Linux/X11**: Uses `x11rb` for X11 protocols including `_NET_WM_STATE`,
//!   `XShape`, `XRecord`, and XDnD.
//! - **Linux/Wayland**: Uses `wayland-client` with `wlr-layer-shell` for
//!   overlay windows and `ext-foreign-toplevel-list` for window observation.

use crate::config::DesktopConfig;
use crate::monitor::MonitorInfo;
use crate::window::WindowDescriptor;
use async_trait::async_trait;
use std::sync::Arc;

/// The platform backend trait abstracts all OS-specific window operations.
/// Implemented separately for macOS, Windows, Linux/X11, and Linux/Wayland.
/// The `DesktopManager` selects the correct backend at startup.
#[async_trait]
pub trait PlatformBackend: Send + Sync {
    /// Platform identifier for diagnostics (e.g., "macos", "windows", "x11", "wayland").
    fn name(&self) -> &'static str;

    /// Create a transparent always-on-top window for the character stage.
    fn create_stage_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError>;

    /// Create a floating overlay panel window (workspace panels, notifications).
    fn create_panel_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError>;

    /// Create a standard bordered settings window.
    fn create_settings_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError>;

    /// Enable or disable click-through (mouse event pass-through) for a window.
    fn set_click_through(
        &self,
        window: &winit::window::Window,
        enabled: bool,
    ) -> Result<(), crate::DesktopError>;

    /// Set the window's position in the OS compositor's Z-stack.
    fn set_z_order(
        &self,
        window: &winit::window::Window,
        order: crate::zorder::ZOrder,
    ) -> Result<(), crate::DesktopError>;

    /// Enable per-pixel alpha compositing (required for transparent windows).
    fn enable_transparency(
        &self,
        window: &winit::window::Window,
    ) -> Result<(), crate::DesktopError>;

    /// Enumerate all connected monitors with full metadata.
    fn enumerate_monitors(
        &self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<Vec<MonitorInfo>, crate::DesktopError>;

    /// Set window opacity (0.0 = fully transparent, 1.0 = fully opaque).
    fn set_window_opacity(
        &self,
        window: &winit::window::Window,
        opacity: f32,
    ) -> Result<(), crate::DesktopError>;
}

// Platform-specific implementations are conditionally compiled.
// Each module must implement PlatformBackend for its target OS.

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform_impl;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform_impl;

#[cfg(all(unix, not(target_os = "macos"), feature = "x11"))]
#[path = "linux_x11.rs"]
mod platform_impl_x11;

#[cfg(all(unix, not(target_os = "macos"), feature = "wayland"))]
#[path = "linux_wayland.rs"]
mod platform_impl_wayland;

/// Create the platform backend for the current OS.
///
/// On Linux, the backend is selected at runtime:
/// 1. If `WAYLAND_DISPLAY` env var is set and the `wayland` feature is enabled,
///    use the Wayland backend.
/// 2. Otherwise, if the `x11` feature is enabled, use the X11 backend.
/// 3. Fall back to a stub backend that returns `UnsupportedPlatform` errors.
///
/// # Errors
/// Returns `DesktopError::UnsupportedPlatform` if no backend is available for
/// the current platform.
pub fn create_backend() -> Result<Box<dyn PlatformBackend>, crate::DesktopError> {
    // macOS
    #[cfg(target_os = "macos")]
    {
        return Ok(Box::new(platform_impl::MacOsBackend::new()));
    }

    // Windows
    #[cfg(target_os = "windows")]
    {
        return Ok(Box::new(platform_impl::WindowsBackend::new()));
    }

    // Linux: runtime selection between X11 and Wayland
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Check for Wayland first.
        #[cfg(feature = "wayland")]
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return Ok(Box::new(platform_impl_wayland::WaylandBackend::new()));
        }

        #[cfg(feature = "x11")]
        {
            return Ok(Box::new(platform_impl_x11::X11Backend::new()));
        }
    }

    // No backend found.
    Err(crate::DesktopError::UnsupportedPlatform {
        platform: std::env::consts::OS,
    })
}

// --- Stub backend for tests ---

/// A test-only backend that records operations in memory.
/// All window creation returns `Err(DesktopError::UnsupportedPlatform)` since
/// tests typically do not have a display connection.
pub struct TestBackend;

#[async_trait]
impl PlatformBackend for TestBackend {
    fn name(&self) -> &'static str {
        "test"
    }

    fn create_stage_window(
        &self,
        _descriptor: &WindowDescriptor,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError> {
        Err(crate::DesktopError::UnsupportedPlatform {
            platform: "test",
        })
    }

    fn create_panel_window(
        &self,
        _descriptor: &WindowDescriptor,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError> {
        Err(crate::DesktopError::UnsupportedPlatform {
            platform: "test",
        })
    }

    fn create_settings_window(
        &self,
        _descriptor: &WindowDescriptor,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, crate::DesktopError> {
        Err(crate::DesktopError::UnsupportedPlatform {
            platform: "test",
        })
    }

    fn set_click_through(
        &self,
        _window: &winit::window::Window,
        _enabled: bool,
    ) -> Result<(), crate::DesktopError> {
        Ok(())
    }

    fn set_z_order(
        &self,
        _window: &winit::window::Window,
        _order: super::zorder::ZOrder,
    ) -> Result<(), crate::DesktopError> {
        Ok(())
    }

    fn enable_transparency(
        &self,
        _window: &winit::window::Window,
    ) -> Result<(), crate::DesktopError> {
        Ok(())
    }

    fn enumerate_monitors(
        &self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<Vec<MonitorInfo>, crate::DesktopError> {
        Ok(Vec::new())
    }

    fn set_window_opacity(
        &self,
        _window: &winit::window::Window,
        _opacity: f32,
    ) -> Result<(), crate::DesktopError> {
        Ok(())
    }
}
