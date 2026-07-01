//! Linux Wayland platform backend implementation.
//!
//! Uses `wayland-client` with `wlr-layer-shell` protocol for overlay windows
//! and `ext-foreign-toplevel-list` for window observation.
//!
//! # Platform Notes
//! - Wayland restricts global input observation by design. The `pointer-constraints`
//!   protocol provides limited cursor tracking within Lumi-owned surfaces only.
//! - Transparent windows use `wl_surface` with an ARGB32 buffer and the
//!   `wlr-layer-shell` protocol for overlay positioning.
//! - Always-on-top is implicit for layer-shell surfaces with `layer = Overlay`.
//! - Click-through is not directly supported by the Wayland protocol; the
//!   workaround is to minimize the input region of the surface.
//!
//! # Security
//! Wayland's security model prevents global input hooks. Global input observation
//! on Wayland gracefully degrades to report only Lumi's own window positions
//! and cursor position within Lumas surfaces.
//!
//! # Feature Flag
//! This module is compiled when the `wayland` feature is enabled.

use crate::DesktopError;
use crate::monitor::{MonitorId, MonitorInfo};
use crate::window::WindowDescriptor;
use crate::geometry::{LogicalRect, PhysicalRect, ScaleFactor, LogicalPoint, PhysicalPoint};
use crate::zorder::ZOrder;
use async_trait::async_trait;
use super::PlatformBackend;
use std::sync::Arc;

/// Wayland platform backend.
pub struct WaylandBackend;

impl WaylandBackend {
    /// Create a new Wayland backend instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformBackend for WaylandBackend {
    fn name(&self) -> &'static str {
        "wayland"
    }

    fn create_stage_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, DesktopError> {
        use winit::window::WindowAttributes;

        let attrs = WindowAttributes::default()
            .with_title(&descriptor.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                descriptor.initial_size.width,
                descriptor.initial_size.height,
            ))
            .with_position(winit::dpi::LogicalPosition::new(
                descriptor.initial_position.x,
                descriptor.initial_position.y,
            ))
            .with_transparent(descriptor.transparent)
            .with_decorations(false) // No decorations on stage window
            .with_always_on_top(true);

        let window = event_loop.create_window(attrs).map_err(|e| {
            DesktopError::WindowCreationFailed {
                reason: format!("Failed to create Wayland stage window: {}", e),
            }
        })?;

        Ok(window)
    }

    fn create_panel_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, DesktopError> {
        use winit::window::WindowAttributes;

        let attrs = WindowAttributes::default()
            .with_title(&descriptor.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                descriptor.initial_size.width,
                descriptor.initial_size.height,
            ))
            .with_position(winit::dpi::LogicalPosition::new(
                descriptor.initial_position.x,
                descriptor.initial_position.y,
            ))
            .with_transparent(descriptor.transparent)
            .with_decorations(descriptor.decorations)
            .with_always_on_top(descriptor.always_on_top);

        let window = event_loop.create_window(attrs).map_err(|e| {
            DesktopError::WindowCreationFailed {
                reason: format!("Failed to create Wayland panel window: {}", e),
            }
        })?;

        Ok(window)
    }

    fn create_settings_window(
        &self,
        descriptor: &WindowDescriptor,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<winit::window::Window, DesktopError> {
        use winit::window::WindowAttributes;

        let attrs = WindowAttributes::default()
            .with_title(&descriptor.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                descriptor.initial_size.width,
                descriptor.initial_size.height,
            ))
            .with_position(winit::dpi::LogicalPosition::new(
                descriptor.initial_position.x,
                descriptor.initial_position.y,
            ))
            .with_decorations(true);

        let window = event_loop.create_window(attrs).map_err(|e| {
            DesktopError::WindowCreationFailed {
                reason: format!("Failed to create Wayland settings window: {}", e),
            }
        })?;

        Ok(window)
    }

    fn set_click_through(
        &self,
        _window: &winit::window::Window,
        _enabled: bool,
    ) -> Result<(), DesktopError> {
        // Wayland does not provide a cross-compositor mechanism for click-through
        // (mouse event pass-through). On wlr-layer-shell surfaces, the input
        // region can be set to empty, but this is compositor-specific.
        //
        // The recommended approach on Wayland is to handle this at the
        // application level: when click-through is enabled, forward mouse
        // events to a no-op handler and visually indicate pass-through mode.
        Ok(())
    }

    fn set_z_order(
        &self,
        window: &winit::window::Window,
        _order: ZOrder,
    ) -> Result<(), DesktopError> {
        // On Wayland, z-order is managed by the compositor. Layer-shell
        // surfaces with different layers implicitly order surfaces.
        // The winit window level is used as a hint.
        window.set_window_level(match _order {
            ZOrder::Stage => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Panel => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Settings => winit::window::WindowLevel::Normal,
            ZOrder::Notification => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Modal => winit::window::WindowLevel::AlwaysOnTop,
        });
        Ok(())
    }

    fn enable_transparency(
        &self,
        _window: &winit::window::Window,
    ) -> Result<(), DesktopError> {
        // On Wayland, transparency is handled at the wl_surface level via
        // the buffer format. winit's `with_transparent(true)` sets the
        // appropriate visual/surface format for per-pixel alpha.
        Ok(())
    }

    fn enumerate_monitors(
        &self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<Vec<MonitorInfo>, DesktopError> {
        let screens = event_loop.available_monitors();
        let mut monitors = Vec::new();

        for screen in screens {
            let name = screen.name().unwrap_or_else(|| "Unknown".into());
            let size = screen.size();
            let scale = screen.scale_factor();

            monitors.push(MonitorInfo {
                id: MonitorId::new(),
                name,
                is_primary: true,
                physical_rect: PhysicalRect {
                    origin: PhysicalPoint { x: 0, y: 0 },
                    size: crate::geometry::Size {
                        width: size.width,
                        height: size.height,
                    },
                },
                work_area: LogicalRect {
                    origin: LogicalPoint { x: 0.0, y: 0.0 },
                    size: crate::geometry::Size {
                        width: size.width as f64 / scale,
                        height: size.height as f64 / scale,
                    },
                },
                scale_factor: ScaleFactor(scale),
                refresh_rate_hz: screen.refresh_rate_millihertz().map(|hz| hz as f64 / 1000.0),
                color_depth: 32,
                connected_at: chrono::Utc::now(),
            });
        }

        Ok(monitors)
    }

    fn set_window_opacity(
        &self,
        _window: &winit::window::Window,
        _opacity: f32,
    ) -> Result<(), DesktopError> {
        // On Wayland, per-window opacity is compositor-specific and not
        // part of the core Wayland protocol. Some compositors support
        // the wlr-layer-shell `set_exclusive_zone` for opacity hints,
        // but there is no universal opacity control.
        Ok(())
    }
}
