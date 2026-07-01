//! # Lumas Desktop Engine
//!
//! The single interface between Lumas and every operating system desktop API.
//! No other Lumas crate may call OS window management, input, or compositor
//! APIs directly.
//!
//! ## Architecture
//!
//! The Desktop Engine owns the winit `EventLoop` on a dedicated thread. All
//! cross-thread requests use `DesktopCommandChannel` with typed commands and
//! `oneshot` responders. Platform-specific operations are delegated to the
//! `PlatformBackend` trait with per-OS implementations.
//!
//! ## Window Topology
//!
//! ```text
//! Desktop (OS compositor)
//! ├── StageWindow — transparent, always-on-top, click-through by default
//! │   └── CharacterViewport — sub-region, hit-tested for mouse interaction
//! ├── WorkspacePanels[] — per-task floating panels
//! └── SettingsWindow — standard bordered window
//! ```
//!
//! ## Platform Support
//! - **macOS 13+**: NSWindow with CoreGraphics display enumeration
//! - **Windows 11**: HWND with layered windows and WinEvent hooks
//! - **Linux X11**: XShape, XRecord, XDnD protocols
//! - **Linux Wayland**: wlr-layer-shell for overlays (limited global input)
//!
//! # WORKSPACE AUDIT
//!
//! ## Dependencies on Other Crates
//!
//! - **lumas-runtime**: Uses `EventBus` for publishing desktop events, `RuntimeContext`
//!   for shared state. The `Event` trait is extended in `events.rs` — we do not
//!   redefine it.
//! - **lumi-logging**: All diagnostics and error reporting use `tracing` macros.
//! - **lumas-config**: `RenderingConfig` mirrors our `DesktopConfig` fields for
//!   DPI, FPS, and window sizing.
//! - **lumi-process**: The Process Manager does not manage windows directly,
//!   but the Desktop Engine's health is reported through `ProcessDiagnostics`.
//!
//! ## Key Design Decisions
//!
//! 1. **Winit EventLoop is !Send**: The event loop runs on a dedicated thread.
//!    All cross-thread communication uses `DesktopCommandChannel` (crossbeam + oneshot).
//! 2. **Alpha mask via shared memory**: The render process writes alpha data to
//!    a shared memory region (`[u32 frame_id][u8 * w * h]`). The Desktop Engine
//!    reads it for hit testing.
//! 3. **Double-buffered hit testing**: Alpha mask updates use a front/back buffer
//!    pattern to avoid data races between the render thread and event loop.
//! 4. **Platform backend trait**: All OS-specific operations are behind
//!    `PlatformBackend`. No platform code leaks into the core modules.
//!
//! # Thread Safety
//! `DesktopManager` is `Send + Sync` and designed to be stored in `RuntimeContext`.
//! All internal state uses `Arc`, `DashMap`, and `ArcSwap` for concurrent access.

pub mod command;
pub mod config;
pub mod diagnostics;
pub mod drag_drop;
pub mod dpi;
pub mod error;
pub mod events;
pub mod geometry;
pub mod hit_test;
pub mod input;
pub mod metrics;
pub mod monitor;
pub mod observer;
pub mod overlay;
pub mod platform;
pub mod render;
pub mod window;
pub mod workspace;
pub mod zorder;

// DesktopManager is the primary public API.
mod manager;
pub use manager::DesktopManager;
pub use error::DesktopError;
pub use geometry::*;
pub use monitor::*;
pub use window::*;
pub use overlay::*;
pub use zorder::*;
pub use config::*;
pub use metrics::*;
pub use render::RenderBridge;

/// Re-export commonly used types at the crate root.
pub use dpi::*;
pub use hit_test::{HitResult, HitTester};
pub use command::DesktopCommandChannel;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_imports() {
        // Verify that core types can be constructed.
        let _ = DesktopManager::new_test(DesktopConfig::default());
        let _ = DesktopError::EventLoopExited;
        let _ = LogicalPoint { x: 0.0, y: 0.0 };
        let _ = PhysicalPoint { x: 0, y: 0 };
        let _ = WindowId::new();
    }
}
