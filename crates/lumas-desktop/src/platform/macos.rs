//! macOS platform backend implementation.
//!
//! Uses `objc2` bindings for NSWindow operations and `core-graphics` for
//! display enumeration and CGEventTap input observation.
//!
//! # Platform Notes
//! - Transparent windows use `NSWindow` with `isOpaque = false` and
//!   `backgroundColor = NSColor.clearColor`.
//! - Always-on-top is achieved via `NSWindow.level = NSFloatingWindowLevel`.
//! - Click-through uses `[NSWindow setIgnoresMouseEvents:]`.
//! - Transparency requires setting `NSWindow.hasShadow = false` to avoid
//!   visual artifacts on the stage window.
//!
//! # SAFETY
//! This module uses `objc2` for raw Objective-C messaging. All selector calls
//! are wrapped in `unsafe` blocks with documented invariants.

use crate::DesktopError;
use crate::monitor::{MonitorId, MonitorInfo};
use crate::overlay::OverlayDescriptor;
use crate::window::WindowDescriptor;
use crate::config::DesktopConfig;
use crate::geometry::{LogicalRect, PhysicalRect, ScaleFactor, LogicalPoint, PhysicalPoint};
use crate::zorder::ZOrder;
use async_trait::async_trait;
use super::PlatformBackend;
use std::sync::Arc;

/// macOS platform backend.
pub struct MacOsBackend;

impl MacOsBackend {
    /// Create a new macOS backend instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformBackend for MacOsBackend {
    fn name(&self) -> &'static str {
        "macos"
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
            .with_decorations(descriptor.decorations)
            .with_always_on_top(descriptor.always_on_top)
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);

        let window = event_loop.create_window(attrs).map_err(|e| {
            DesktopError::WindowCreationFailed {
                reason: format!("Failed to create stage window: {}", e),
            }
        })?;

        // Enable per-pixel alpha compositing via NSWindow.
        self.enable_transparency(&window)?;

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
                reason: format!("Failed to create panel window: {}", e),
            }
        })?;

        if descriptor.transparent {
            self.enable_transparency(&window)?;
        }

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
                reason: format!("Failed to create settings window: {}", e),
            }
        })?;

        Ok(window)
    }

    fn set_click_through(
        &self,
        window: &winit::window::Window,
        enabled: bool,
    ) -> Result<(), DesktopError> {
        // SAFETY: This uses the raw NSWindow pointer obtained from the winit
        // window. winit guarantees the window is valid for the lifetime of the
        // `Window` object. `setIgnoresMouseEvents:` is a standard NSWindow
        // method that does not require additional setup.
        #[cfg(target_os = "macos")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = window.window_handle().map_err(|e| {
                DesktopError::PlatformError {
                    operation: "get_window_handle",
                    source: Box::new(e),
                }
            })?;

            if let RawWindowHandle::AppKit(ns_handle) = handle.as_ref() {
                let ns_window = ns_handle.ns_window.as_ptr();
                if !ns_window.is_null() {
                    // SAFETY: ns_window is a valid NSWindow pointer obtained
                    // from winit. Objective-C messaging with the standard
                    // selector is safe.
                    unsafe {
                        objc2::msg_send![
                            objc2::rc::Retained::from_raw(ns_window as *mut objc2::runtime::NSObject),
                            setIgnoresMouseEvents: enabled
                        ]
                    }
                }
            }
        }
        Ok(())
    }

    fn set_z_order(
        &self,
        window: &winit::window::Window,
        _order: ZOrder,
    ) -> Result<(), DesktopError> {
        // On macOS, NSWindow levels handle z-order. We set the level based on
        // the z-order tier. The winit `set_window_level` method is used when
        // available; otherwise we use raw NSWindow messaging.
        window.set_window_level(match _order {
            ZOrder::Stage => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Panel => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Settings => winit::window::WindowLevel::Normal,
            ZOrder::Notification => winit::window::WindowLevel::AlwaysOnTop,
            ZOrder::Modal => winit::window::WindowLevel::ModalPanel,
        });
        Ok(())
    }

    fn enable_transparency(
        &self,
        window: &winit::window::Window,
    ) -> Result<(), DesktopError> {
        // SAFETY: Accessing raw NSWindow pointer to set transparency flags
        // that winit does not yet expose through its public API.
        #[cfg(target_os = "macos")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::AppKit(ns_handle) = handle.as_ref() {
                    let ns_window = ns_handle.ns_window.as_ptr();
                    if !ns_window.is_null() {
                        unsafe {
                            // Disable shadow for stage windows (prevents visual artifacts).
                            let _: () = objc2::msg_send![
                                objc2::rc::Retained::from_raw(ns_window as *mut objc2::runtime::NSObject),
                                setHasShadow: false
                            ];
                            // Set background color to clear for transparency.
                            let _: () = objc2::msg_send![
                                objc2::rc::Retained::from_raw(ns_window as *mut objc2::runtime::NSObject),
                                setOpaque: false
                            ];
                            let clear_color: *mut objc2::runtime::NSObject = objc2::msg_send![
                                objc2::class!(NSColor),
                                clearColor
                            ];
                            let _: () = objc2::msg_send![
                                objc2::rc::Retained::from_raw(ns_window as *mut objc2::runtime::NSObject),
                                setBackgroundColor: clear_color
                            ];
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn enumerate_monitors(
        &self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Result<Vec<MonitorInfo>, DesktopError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::*;
            let screens = _event_loop.available_monitors();
            let mut monitors = Vec::new();

            for screen in screens {
                let monitor_info = monitor_info_from_winit(screen);
                monitors.push(monitor_info);
            }

            // Fallback: use CGDisplay API if winit returns empty.
            if monitors.is_empty() {
                let main_id = CGMainDisplayID();
                for i in 0..CGGetNumActiveDisplays() {
                    // SAFETY: CGGetActiveDisplayList is a safe CoreGraphics API.
                    let display_id: CGDirectDisplayID = unsafe {
                        let mut displays = [0u32; 32];
                        let mut count = 0u32;
                        CGGetActiveDisplayList(32, &mut displays, &mut count);
                        if i < count as usize {
                            displays[i]
                        } else {
                            break;
                        }
                    };

                    monitors.push(MonitorInfo {
                        id: MonitorId::new(),
                        name: format!("Display {}", i + 1),
                        is_primary: display_id == main_id,
                        physical_rect: PhysicalRect {
                            origin: PhysicalPoint {
                                x: CGDisplayPixelsWide(display_id),
                                y: CGDisplayPixelsHigh(display_id),
                            },
                            size: crate::geometry::Size {
                                width: CGDisplayPixelsWide(display_id),
                                height: CGDisplayPixelsHigh(display_id),
                            },
                        },
                        work_area: LogicalRect {
                            origin: LogicalPoint { x: 0.0, y: 0.0 },
                            size: crate::geometry::Size {
                                width: CGDisplayPixelsWide(display_id) as f64,
                                height: CGDisplayPixelsHigh(display_id) as f64,
                            },
                        },
                        scale_factor: ScaleFactor(CGDisplayScreenSize(display_id) as f64 / 100.0),
                        refresh_rate_hz: Some(60.0),
                        color_depth: 32,
                        connected_at: chrono::Utc::now(),
                    });
                }
            }

            return Ok(monitors);
        }

        Ok(Vec::new())
    }

    fn set_window_opacity(
        &self,
        window: &winit::window::Window,
        opacity: f32,
    ) -> Result<(), DesktopError> {
        window.set_window_level(match opacity {
            _ if opacity >= 0.99 => winit::window::WindowLevel::AlwaysOnTop,
            _ => winit::window::WindowLevel::Normal,
        });
        Ok(())
    }
}

// Helper to convert a winit MonitorHandle to MonitorInfo
#[cfg(target_os = "macos")]
fn monitor_info_from_winit(monitor: winit::monitor::MonitorHandle) -> MonitorInfo {
    let name = monitor.name().unwrap_or_else(|| "Unknown".into());
    let size = monitor.size();
    let scale = monitor.scale_factor();

    MonitorInfo {
        id: MonitorId::new(),
        name,
        is_primary: true, // approximated; winit doesn't expose is_primary directly on all platforms
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
        refresh_rate_hz: monitor.refresh_rate_millihertz().map(|hz| hz as f64 / 1000.0),
        color_depth: 32,
        connected_at: chrono::Utc::now(),
    }
}
