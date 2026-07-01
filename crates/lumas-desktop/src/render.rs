//! Render bridge — integrates the lumas-render GPU engine with the winit window.
//!
//! The `RenderBridge` owns the `lumas_render::Renderer` and connects it to the
//! desktop window lifecycle:
//!
//! 1. **Window handle** — A `RawWindowHandle` from the winit window is passed
//!    to `Renderer::new()` at construction time for wgpu surface creation.
//! 2. **Render loop** — `render_frame()` drives the full pipeline (begin frame,
//!    upload uniforms, compile graph, execute passes, present).
//! 3. **Resize** — `resize()` reconfigures the surface and framebuffers when
//!    the window size changes.
//! 4. **Scene updates** — The `Scene` is updated by the IPC layer and read by
//!    the renderer each frame.
//!
//! # Thread Safety
//!
//! `RenderBridge` requires exclusive access from the render thread. It is
//! `Send` but not `Sync`. All operations must go through the event loop thread
//! or a dedicated render thread.
//!
//! # Lifecycle
//!
//! 1. Create a `RenderConfig` from `DesktopConfig` via `RenderConfig::from_desktop()`.
//! 2. Await `RenderBridge::new(config, raw_handle)` to create the renderer.
//! 3. Call `render_frame()` from the event loop's `RedrawRequested` handler.
//! 4. Call `resize()` when the winit window resizes.
//! 5. Call `shutdown()` to drop the renderer gracefully.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Creates a `lumas_render::RenderConfig` from a `DesktopConfig`.
///
/// Maps:
/// - `stage_width` / `stage_height` → `surface_width` / `surface_height`
/// - `vsync_enabled` → `present_mode` (Fifo vs Immediate)
/// - `composite_alpha` → always `PreMultiplied` (transparent window)
pub fn render_config_from_desktop(config: &crate::config::DesktopConfig) -> lumas_render::RenderConfig {
    let mut render_config = lumas_render::RenderConfig::default();

    render_config.surface_width = config.stage_width.max(1.0) as u32;
    render_config.surface_height = config.stage_height.max(1.0) as u32;
    render_config.present_mode = if config.vsync_enabled {
        lumas_render::config::PresentMode::Fifo
    } else {
        lumas_render::config::PresentMode::Immediate
    };
    render_config.composite_alpha = lumas_render::config::CompositeAlphaMode::PreMultiplied;

    render_config
}

/// The render bridge — owns the `lumas_render::Renderer` and drives its lifecycle.
pub struct RenderBridge {
    /// The lumas-render renderer.
    renderer: lumas_render::Renderer,
    /// The current scene (updated by IPC, read by render_frame).
    scene: lumas_render::Scene,
    /// Whether the renderer should stop rendering.
    shutting_down: Arc<AtomicBool>,
}

impl RenderBridge {
    /// Create a new render bridge with the given config and window handle.
    ///
    /// This is async because it creates the GPU adapter, device, and surface.
    ///
    /// # Errors
    /// Returns `lumas_render::RenderError` if GPU initialization fails.
    pub async fn new(
        config: &lumas_render::RenderConfig,
        raw_handle: &raw_window_handle::RawWindowHandle,
    ) -> Result<Self, lumas_render::RenderError> {
        let mut renderer = lumas_render::Renderer::new(config, Some(raw_handle)).await?;

        // Register all standard render passes.
        renderer.initialize_passes()?;

        Ok(Self {
            renderer,
            scene: lumas_render::Scene::new(),
            shutting_down: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Drive one frame of the render pipeline.
    ///
    /// Call this from the event loop's `RedrawRequested` or `AboutToWait` handler.
    /// Internally calls `render_frame()` which handles begin/upload/compile/execute/end.
    ///
    /// # Errors
    /// Returns `lumas_render::RenderError` if any render stage fails.
    /// The caller should log the error and potentially retry on the next frame.
    pub fn render_frame(&mut self) -> Result<(), lumas_render::RenderError> {
        if self.shutting_down.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Update scene time.
        self.scene.time_seconds += 1.0 / 60.0;

        // The renderer reads from its internal scene reference.
        // Set the scene before each frame render.
        self.renderer.set_scene(self.scene.clone());
        self.renderer.render_frame()
    }

    /// Resize the rendering surface and framebuffers.
    ///
    /// Call this when the winit window fires a `Resized` event.
    ///
    /// # GPU Thread Safety
    /// Must be called from the render thread.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    /// Get a mutable reference to the scene for IPC updates.
    ///
    /// The scene is cloned before being set on the renderer each frame,
    /// so writes to this scene are picked up on the next `render_frame()` call.
    pub fn scene_mut(&mut self) -> &mut lumas_render::Scene {
        &mut self.scene
    }

    /// Get a reference to the scene.
    pub fn scene(&self) -> &lumas_render::Scene {
        &self.scene
    }

    /// Get a reference to the underlying renderer.
    pub fn renderer(&self) -> &lumas_render::Renderer {
        &self.renderer
    }

    /// Get a mutable reference to the underlying renderer.
    pub fn renderer_mut(&mut self) -> &mut lumas_render::Renderer {
        &mut self.renderer
    }

    /// Get the shutdown flag for coordination with the event loop.
    pub fn shutting_down(&self) -> Arc<AtomicBool> {
        self.shutting_down.clone()
    }

    /// Initiate graceful shutdown.
    pub fn shutdown(&mut self) {
        self.shutting_down.store(true, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for RenderBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderBridge")
            .field("renderer", &self.renderer)
            .field("shutting_down", &self.shutting_down.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_config_from_desktop() {
        let desktop_config = crate::config::DesktopConfig::default();
        let render_config = render_config_from_desktop(&desktop_config);
        assert!(render_config.surface_width > 0);
        assert!(render_config.surface_height > 0);
        assert_eq!(
            render_config.composite_alpha,
            lumas_render::config::CompositeAlphaMode::PreMultiplied
        );
    }

    #[test]
    fn test_render_config_vsync_mapping() {
        let mut config = crate::config::DesktopConfig::default();
        config.vsync_enabled = true;
        let render_config = render_config_from_desktop(&config);
        assert_eq!(render_config.present_mode, lumas_render::config::PresentMode::Fifo);

        config.vsync_enabled = false;
        let render_config = render_config_from_desktop(&config);
        assert_eq!(render_config.present_mode, lumas_render::config::PresentMode::Immediate);
    }

    #[test]
    fn test_render_bridge_type_check() {
        // Without a GPU we can't create a real bridge,
        // but verify the type exists and config conversion works.
        let desktop_config = crate::config::DesktopConfig::default();
        let render_config = render_config_from_desktop(&desktop_config);
        assert_eq!(render_config.surface_width, 400);
        assert_eq!(render_config.surface_height, 600);
    }
}
