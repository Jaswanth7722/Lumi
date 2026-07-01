//! Windows platform backend implementation.
//!
//! Uses the `windows` crate for HWND manipulation, layered window attributes,
//! WinEvent hooks for accessibility observation, and low-level keyboard/mouse
//! hooks for global input observation.
//!
//! # Platform Notes
//! - Transparent windows use `WS_EX_LAYERED` extended style with
//!   `SetLayeredWindowAttributes` for per-pixel alpha.
//! - Always-on-top is achieved via `WS_EX_TOPMOST`.
//! - Click-through uses `WS_EX_TRANSPARENT` extended style.
//! - WinEvent hooks (`SetWinEventHook`) provide window observation.
//! - Low-level hooks (`SetWindowsHookEx(WH_MOUSE_LL, WH_KEYBOARD_LL)`) provide
//!   global input observation without consuming events.

use crate::DesktopError;
use crate::monitor::{MonitorId, MonitorInfo};
use crate::window::WindowDescriptor;
use crate::geometry::{LogicalRect, PhysicalRect, ScaleFactor, LogicalPoint, PhysicalPoint};
use crate::zorder::ZOrder;
use async_trait::async_trait;
use super::PlatformBackend;
use std::sync::Arc;

/// Windows platform backend.
pub struct WindowsBackend;

impl WindowsBackend {
    /// Create a new Windows backend instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformBackend for WindowsBackend {
    fn name(&self) -> &'static str {
        "windows"
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
            .with_always_on_top(true);

        let window = event_loop.create_window(attrs).map_err(|e| {
            DesktopError::WindowCreationFailed {
                reason: format!("Failed to create stage window: {}", e),
            }
        })?;

        // Apply WS_EX_LAYERED for per-pixel alpha compositing.
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
        // On Windows, click-through is achieved by toggling the
        // WS_EX_TRANSPARENT extended style. We also toggle WS_EX_LAYERED
        // as WS_EX_TRANSPARENT requires it.
        //
        // SAFETY: We access the raw HWND via raw-window-handle. winit guarantees
        // the HWND is valid for the lifetime of the Window object.
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(win_handle) = handle.as_ref() {
                    let hwnd = win_handle.hwnd.get();
                    if !hwnd.is_null() {
                        unsafe {
                            use windows::Win32::UI::WindowsAndMessaging::{
                                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
                                WS_EX_LAYERED, WS_EX_TRANSPARENT,
                            };

                            let current_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                            if enabled {
                                // Add WS_EX_TRANSPARENT (pass-through mouse events).
                                let new_style = current_style | WS_EX_TRANSPARENT.0 as isize;
                                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
                            } else {
                                // Remove WS_EX_TRANSPARENT (accept mouse events).
                                let new_style = current_style & !(WS_EX_TRANSPARENT.0 as isize);
                                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
                            }
                        }
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
        // On Windows, z-order is managed through SetWindowPos with HWND_TOPMOST,
        // HWND_TOP, or HWND_NOTOPMOST. winit's set_window_level is the primary
        // mechanism; we use raw HWND for the WS_EX_TOPMOST flag.
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
        window: &winit::window::Window,
    ) -> Result<(), DesktopError> {
        // On Windows, per-pixel transparency requires the WS_EX_LAYERED
        // extended style. winit's `with_transparent` handles this in most
        // cases, but we ensure it's set.
        //
        // SAFETY: HWND is obtained from raw-window-handle and is guaranteed
        // valid by winit.
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(win_handle) = handle.as_ref() {
                    let hwnd = win_handle.hwnd.get();
                    if !hwnd.is_null() {
                        unsafe {
                            use windows::Win32::UI::WindowsAndMessaging::{
                                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
                                WS_EX_LAYERED,
                            };

                            let current_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                            if current_style & WS_EX_LAYERED.0 as isize == 0 {
                                let new_style = current_style | WS_EX_LAYERED.0 as isize;
                                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
                            }
                        }
                    }
                }
            }
        }
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
                is_primary: true, // approximated; winit doesn't expose is_primary on all platforms
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
        window: &winit::window::Window,
        opacity: f32,
    ) -> Result<(), DesktopError> {
        // On Windows, per-window opacity is set via SetLayeredWindowAttributes
        // which requires WS_EX_LAYERED to be set first.
        let _ = self.enable_transparency(window)?;

        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(win_handle) = handle.as_ref() {
                    let hwnd = win_handle.hwnd.get();
                    if !hwnd.is_null() {
                        unsafe {
                            use windows::Win32::UI::WindowsAndMessaging::{
                                SetLayeredWindowAttributes, LWA_ALPHA,
                            };
                            let alpha_byte = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
                            let _ = SetLayeredWindowAttributes(
                                windows::Win32::Foundation::HWND(hwnd),
                                0,
                                alpha_byte,
                                LWA_ALPHA,
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
