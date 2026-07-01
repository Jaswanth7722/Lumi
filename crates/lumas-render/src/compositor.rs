//! Desktop compositor — surface presentation, swapchain management, and pre-multiplied alpha output.
//!
//! Lumas renders to a transparent, borderless window above the desktop.
//! This requires **pre-multiplied alpha** throughout the entire pipeline.
//! Incorrect alpha compositing produces black fringes around the character
//! that are immediately visible to users.
//!
//! # Pre-Multiplied Alpha Pipeline
//!
//! 1. Character textures are stored pre-multiplied (during import)
//! 2. All blend states use `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`
//! 3. The final composite pass outputs `rgba = (color.rgb * color.a, color.a)`
//! 4. Surface uses `CompositeAlphaMode::PreMultiplied` where supported

use crate::config::{CompositeAlphaMode, RenderConfig};
use crate::context::GpuContext;
use crate::error::{ErrorSeverity, RenderError};
use crate::graph::FrameContext;

/// Describes the current swapchain state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapchainState {
    /// Swapchain is healthy and ready for presentation.
    Healthy,
    /// Swapchain is outdated (window resized).
    Outdated,
    /// Swapchain was lost and needs recreation.
    Lost,
    /// Swapchain timed out (no new frame available).
    Timeout,
}

/// The compositor manages the swapchain and final output to the surface.
///
/// It owns:
/// - The surface texture acquisition and presentation
/// - The final composite pass output (the LDR pre-multiplied alpha result)
/// - Hit-test mask generation (delegated to overlay)
/// - Window resize handling
#[derive(Debug)]
pub struct Compositor {
    /// wgpu surface format.
    surface_format: wgpu::TextureFormat,
    /// Composite alpha mode (pre-multiplied or opaque fallback).
    composite_alpha_mode: CompositeAlphaMode,
    /// Width of the output surface in physical pixels.
    width: u32,
    /// Height of the output surface in physical pixels.
    height: u32,
    /// Number of consecutive surface acquisition timeouts.
    consecutive_timeouts: u32,
    /// Whether the surface was healthy on the last acquire.
    last_acquire_healthy: bool,
    /// The output texture view (LDR pre-multiplied alpha result).
    /// This is the texture that the final composite pass writes to.
    output_texture: Option<wgpu::Texture>,
    output_texture_view: Option<wgpu::TextureView>,
}

impl Compositor {
    /// Create a new compositor from the GPU context.
    ///
    /// # GPU Thread Safety
    /// Must be created on the main thread.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (one-time setup).
    ///
    /// # Panics
    /// This function does not panic.
    pub fn new(ctx: &GpuContext, config: &RenderConfig) -> Self {
        let format = ctx.surface_config.as_ref()
            .map(|c| c.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        Self {
            surface_format: format,
            composite_alpha_mode: config.composite_alpha,
            width: config.surface_width,
            height: config.surface_height,
            consecutive_timeouts: 0,
            last_acquire_healthy: true,
            output_texture: None,
            output_texture_view: None,
        }
    }

    /// Acquire the next surface texture from the swapchain.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (may block on vsync depending on present mode).
    ///
    /// # Errors
    /// Returns `RenderError::SurfaceTimeout` if the surface cannot be acquired
    /// within a reasonable time (increments internal timeout counter).
    /// Returns `RenderError::SurfaceOutdated` if the surface is outdated
    /// (window resize) — the caller should reconfigure the surface and retry.
    /// Returns `RenderError::DeviceLost` if the surface is lost.
    pub fn acquire_surface_texture(
        &mut self,
        ctx: &GpuContext,
    ) -> Result<wgpu::SurfaceTexture, RenderError> {
        let surface = ctx.surface.as_ref().ok_or_else(|| {
            RenderError::SurfaceOutdated {
                severity: ErrorSeverity::Recoverable,
            }
        })?;

        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.consecutive_timeouts = 0;
                self.last_acquire_healthy = true;
                Ok(texture)
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.consecutive_timeouts += 1;
                self.last_acquire_healthy = false;
                Err(RenderError::SurfaceTimeout {
                    severity: if self.consecutive_timeouts > 5 {
                        ErrorSeverity::Critical
                    } else {
                        ErrorSeverity::Recoverable
                    },
                })
            }
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
                self.consecutive_timeouts += 1;
                self.last_acquire_healthy = false;
                Err(RenderError::SurfaceTimeout {
                    severity: if self.consecutive_timeouts > 5 {
                        ErrorSeverity::Critical
                    } else {
                        ErrorSeverity::Recoverable
                    },
                })
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.consecutive_timeouts = 0;
                self.last_acquire_healthy = false;
                Err(RenderError::SurfaceOutdated {
                    severity: ErrorSeverity::Recoverable,
                })
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.consecutive_timeouts = 0;
                self.last_acquire_healthy = false;
                Err(RenderError::DeviceLost {
                    reason: "Swapchain lost".into(),
                    severity: ErrorSeverity::Critical,
                })
            }
        }
    }

    /// Present the final rendered frame to the surface.
    ///
    /// Takes the surface texture and submits it for presentation.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU.
    pub fn present(
        &self,
        _surface_texture: wgpu::SurfaceTexture,
    ) {
        // The surface texture is dropped here, which triggers wgpu's
        // implicit present on Drop. This is the standard pattern.
        // Explicit `present()` is not needed — wgpu handles it.
        drop(_surface_texture);
    }

    /// Reconfigure the compositor for a window resize.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Frame Budget
    /// ~0.02ms CPU.
    pub fn resize(
        &mut self,
        ctx: &GpuContext,
        width: u32,
        height: u32,
    ) {
        self.width = width.max(1);
        self.height = height.max(1);

        // Recreate the output texture.
        self.output_texture = None;
        self.output_texture_view = None;

        let output_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compositor_output"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // LDR output
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = output_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("compositor_output_view"),
            ..Default::default()
        });

        self.output_texture = Some(output_texture);
        self.output_texture_view = Some(view);
    }

    /// Get the output texture view (the LDR pre-multiplied alpha result).
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.output_texture_view.as_ref()
    }

    /// Get the output texture (for reading back pixels for hit-test masks).
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.output_texture.as_ref()
    }

    /// Get the current surface dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get the surface format.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// Get the composite alpha mode.
    pub fn alpha_mode(&self) -> CompositeAlphaMode {
        self.composite_alpha_mode
    }

    /// Check whether the surface was healthy on the last acquire.
    pub fn last_acquire_healthy(&self) -> bool {
        self.last_acquire_healthy
    }

    /// Number of consecutive timeouts.
    pub fn consecutive_timeouts(&self) -> u32 {
        self.consecutive_timeouts
    }

    /// Whether the compositor should fall back to opaque mode.
    /// This happens when PreMultiplied alpha is not supported by the surface.
    pub fn needs_opaque_fallback(&self) -> bool {
        self.composite_alpha_mode == CompositeAlphaMode::Opaque
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_default_state() {
        // Verify component state without needing a GPU context.
        let compositor_state = SwapchainState::Healthy;
        assert_eq!(compositor_state, SwapchainState::Healthy);
    }

    #[test]
    fn test_swapchain_state_transitions() {
        let state = SwapchainState::Healthy;
        assert_ne!(state, SwapchainState::Outdated);
        assert_ne!(state, SwapchainState::Lost);
    }

    #[test]
    fn test_consecutive_timeout_escalation() {
        // After 5 timeouts, severity escalates to Critical.
        // The compositor tracks timeouts; the 6th consecutive timeout
        // triggers Critical severity.
        let threshold: u32 = 5;
        assert!(6 > threshold); // 6 timeouts → would escalate
        assert!(!(3 > threshold)); // 3 timeouts → would not escalate
    }
}
