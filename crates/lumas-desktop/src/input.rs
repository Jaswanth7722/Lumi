//! # Global Input Observation
//!
//! Observes global mouse position and keyboard modifiers without consuming
//! events — other applications still receive input normally.
//!
//! This is observation only. Lumas does not inject synthetic input events.
//! The stage window receives its own mouse events through normal OS delivery.
//!
//! # Thread Safety
//!
//! `GlobalInputObserver` is `Send + Sync`. Cursor position reads are lock-free
//! via `ArcSwap`. Modifier state uses `AtomicU8`.

use crate::error::DesktopError;
use crate::geometry::LogicalPoint;
use crate::metrics::DesktopMetrics;
use arc_swap::ArcSwap;
use lumas_runtime::event::EventBus;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Bitmask flags for modifier keys.
const MOD_SHIFT: u8 = 1 << 0;
const MOD_CTRL: u8 = 1 << 1;
const MOD_ALT: u8 = 1 << 2;
const MOD_SUPER: u8 = 1 << 3;

/// Current state of keyboard modifiers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierState {
    /// Shift key is held.
    pub shift: bool,
    /// Control key is held.
    pub ctrl: bool,
    /// Alt key is held.
    pub alt: bool,
    /// Super/Windows/Command key is held.
    pub super_key: bool,
}

/// Observes global cursor position and keyboard modifiers.
pub struct GlobalInputObserver {
    /// Current cursor position in logical screen coordinates.
    cursor_position: ArcSwap<LogicalPoint>,
    /// Bitmask of pressed modifier keys.
    pressed_modifiers: AtomicU8,
    /// Event bus for emitting input events.
    event_bus: Arc<EventBus>,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

impl GlobalInputObserver {
    /// Create a new global input observer.
    pub fn new(event_bus: Arc<EventBus>, metrics: Arc<DesktopMetrics>) -> Self {
        Self {
            cursor_position: ArcSwap::new(Arc::new(LogicalPoint::new(0.0, 0.0))),
            pressed_modifiers: AtomicU8::new(0),
            event_bus,
            metrics,
        }
    }

    /// Returns the current cursor position in logical screen coordinates.
    ///
    /// Updated on every mouse move event. O(1) — just an Arc load.
    pub fn cursor_position(&self) -> LogicalPoint {
        *self.cursor_position.load_full()
    }

    /// Update the cursor position (called by the platform backend).
    pub(crate) fn update_cursor_position(&self, pos: LogicalPoint) {
        self.cursor_position.store(Arc::new(pos));
    }

    /// Returns currently held modifier keys.
    pub fn modifiers(&self) -> ModifierState {
        let bits = self.pressed_modifiers.load(Ordering::Acquire);
        ModifierState {
            shift: bits & MOD_SHIFT != 0,
            ctrl: bits & MOD_CTRL != 0,
            alt: bits & MOD_ALT != 0,
            super_key: bits & MOD_SUPER != 0,
        }
    }

    /// Update modifier state (called by the platform backend).
    pub(crate) fn update_modifiers(&self, mods: ModifierState) {
        let mut bits = 0u8;
        if mods.shift {
            bits |= MOD_SHIFT;
        }
        if mods.ctrl {
            bits |= MOD_CTRL;
        }
        if mods.alt {
            bits |= MOD_ALT;
        }
        if mods.super_key {
            bits |= MOD_SUPER;
        }
        self.pressed_modifiers.store(bits, Ordering::Release);
    }

    /// Start the global input hook.
    ///
    /// Platform implementations:
    /// - **macOS**: CGEventTap (requires Accessibility permission)
    /// - **Windows**: SetWindowsHookEx(WH_MOUSE_LL, WH_KEYBOARD_LL)
    /// - **Linux/X11**: XRecord extension
    /// - **Linux/Wayland**: Limited — Wayland restricts global input by design
    ///
    /// # Errors
    ///
    /// Returns `DesktopError::InputHookFailed` if the hook cannot be installed.
    /// Returns `DesktopError::PermissionDenied` if accessibility permission is needed.
    pub async fn start(self: Arc<Self>) -> Result<(), DesktopError> {
        // Platform-specific input hook installation is handled by the
        // platform backend. This method signals that the observer is
        // ready to receive events.
        Ok(())
    }

    /// Stop the input hook and release OS resources.
    pub async fn stop(&self) -> Result<(), DesktopError> {
        Ok(())
    }
}

impl std::fmt::Debug for GlobalInputObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalInputObserver").finish()
    }
}
