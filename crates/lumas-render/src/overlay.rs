//! Overlay and hit-test mask management.
//!
//! Lumas renders to a transparent desktop window. The Desktop Engine needs
//! to know which pixels have content (clickable) and which are transparent
//! (click-through). This module generates a **hit-test mask** from the final
//! composited alpha channel.
//!
//! # Hit-Test Mask Update
//!
//! After every frame, the overlay system reads the alpha channel of the
//! composited output and updates a mask bitmap. The update is asynchronous:
//! the mask is submitted via `queue.on_submitted_work_done()` callback,
//! never blocking the render loop.
//!
//! The mask is a monochrome bitmap: 1 = opaque (clickable), 0 = transparent (click-through).
//! A pixel is considered "clickable" if its alpha exceeds a threshold (default 0.05).

use crate::compositor::Compositor;
use crate::context::GpuContext;
use crate::error::{ErrorSeverity, RenderError};
use crate::graph::FrameContext;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Alpha threshold for hit-test mask generation.
/// Pixels with alpha above this value are considered clickable.
const HIT_TEST_ALPHA_THRESHOLD: u8 = 13; // ~0.05 * 255

/// Default mask dimensions (will be updated from surface size).
const DEFAULT_MASK_SIZE: usize = 1024;

/// A single row in the hit-test mask (bits packed for compact storage).
#[derive(Debug, Clone)]
pub struct HitTestMask {
    /// Width of the mask in pixels.
    width: u32,
    /// Height of the mask in pixels.
    height: u32,
    /// Packed bitmask: 1 = clickable, 0 = transparent.
    bits: Vec<u8>,
}

impl HitTestMask {
    /// Create a new empty hit-test mask.
    fn new(width: u32, height: u32) -> Self {
        let row_size = ((width + 7) / 8) as usize;
        Self {
            width,
            height,
            bits: vec![0u8; row_size * height as usize],
        }
    }

    /// Set a pixel in the mask.
    fn set_pixel(&mut self, x: u32, y: u32, opaque: bool) {
        if x >= self.width || y >= self.height {
            return;
        }
        let row_size = ((self.width + 7) / 8) as u32;
        let byte_index = (y * row_size + x / 8) as usize;
        let bit_index = x % 8;
        if byte_index < self.bits.len() {
            if opaque {
                self.bits[byte_index] |= 1 << bit_index;
            } else {
                self.bits[byte_index] &= !(1 << bit_index);
            }
        }
    }

    /// Test whether a pixel is clickable (opaque).
    pub fn is_opaque(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let row_size = ((self.width + 7) / 8) as u32;
        let byte_index = (y * row_size + x / 8) as usize;
        let bit_index = x % 8;
        if byte_index < self.bits.len() {
            (self.bits[byte_index] >> bit_index) & 1 == 1
        } else {
            false
        }
    }

    /// Get the raw bitmask data.
    pub fn raw_bits(&self) -> &[u8] {
        &self.bits
    }

    /// Total size of the bitmask in bytes.
    pub fn byte_size(&self) -> usize {
        self.bits.len()
    }

    /// Resize the mask.
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let row_size = ((width + 7) / 8) as usize;
        self.bits = vec![0u8; row_size * height as usize];
    }
}



/// Composite alpha mode (mirrors config).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayCompositeMode {
    /// Pre-multiplied alpha (native surface support).
    PreMultiplied,
    /// Opaque fallback (compositor handles alpha).
    Opaque,
}

/// The overlay renderer manages hit-test mask generation.
///
/// It reads the final composited frame's alpha channel and produces
/// a bitmap mask that the Desktop Engine uses for mouse hit-testing.
/// On transparent windows, this is how Lumas achieves click-through
/// on transparent pixels while remaining clickable on opaque pixels.
#[derive(Debug)]
pub struct OverlayRenderer {
    /// The current hit-test mask.
    mask: HitTestMask,
    /// Whether the mask has been updated since the last frame.
    mask_dirty: Arc<AtomicBool>,
    /// Mask version counter (increments on each update).
    mask_version: Arc<AtomicU32>,
    /// The composite alpha mode.
    composite_mode: OverlayCompositeMode,
    /// Whether a mask readback is pending.
    readback_pending: bool,
}

impl OverlayRenderer {
    /// Create a new overlay renderer.
    ///
    /// # GPU Thread Safety
    /// Must be created on the main thread.
    ///
    /// # Frame Budget
    /// ~0.01ms CPU (one-time setup).
    ///
    /// # Panics
    /// This function does not panic.
    pub fn new(width: u32, height: u32, composite_mode: OverlayCompositeMode) -> Self {
        Self {
            mask: HitTestMask::new(width, height),
            mask_dirty: Arc::new(AtomicBool::new(false)),
            mask_version: Arc::new(AtomicU32::new(0)),
            composite_mode,
            readback_pending: false,
        }
    }

    /// Update the hit-test mask from the final composited alpha channel.
    ///
    /// This is the primary method called after each frame is rendered.
    /// It reads the alpha channel of the output texture and generates
    /// the hit-test mask.
    ///
    /// In production, this would use a GPU readback pipeline:
    /// 1. Copy the alpha channel to a staging buffer
    /// 2. Map the staging buffer
    /// 3. Generate the hit-test mask on the CPU
    ///
    /// For now, we use a simplified path that reads the compositor's
    /// output texture.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    ///
    /// # Frame Budget
    /// ~0.1ms CPU (depends on output size).
    ///
    /// # Errors
    /// Returns `RenderError::BufferMapFailed` if the readback buffer cannot be mapped.
    pub fn update_mask(
        &mut self,
        _ctx: &GpuContext,
        _compositor: &Compositor,
    ) -> Result<(), RenderError> {
        // In a production implementation, this would:
        // 1. Copy the output texture's alpha channel to a staging buffer
        // 2. Map the staging buffer (async)
        // 3. On map completion, generate the hit-test mask
        // 4. Set mask_dirty to true and increment mask_version
        //
        // For now, we skip the readback and maintain a clean mask.
        // The real readback will be implemented when the Desktop Engine
        // integration is built.

        self.readback_pending = false;
        Ok(())
    }

    /// Schedule an async readback of the alpha channel for hit-test mask update.
    ///
    /// This is called after the render graph executes. The actual mask
    /// generation happens in the `on_submitted_work_done` callback.
    ///
    /// # GPU Thread Safety
    /// Callable from render thread only.
    pub fn schedule_readback(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _compositor: &Compositor,
    ) {
        if self.readback_pending {
            return;
        }

        // In production: copy output texture alpha to staging buffer.
        // For now, mark readback as pending — the actual copy will be
        // implemented when the Desktop Engine provides the readback surface.

        self.readback_pending = true;
    }

    /// Get the current hit-test mask.
    pub fn mask(&self) -> &HitTestMask {
        &self.mask
    }

    /// Get the mask version (changes on every update).
    pub fn mask_version(&self) -> u32 {
        self.mask_version.load(Ordering::Relaxed)
    }

    /// Check if the mask has been updated since the last check.
    pub fn is_mask_dirty(&self) -> bool {
        self.mask_dirty.load(Ordering::Relaxed)
    }

    /// Clear the dirty flag.
    pub fn clear_mask_dirty(&self) {
        self.mask_dirty.store(false, Ordering::Relaxed);
    }

    /// Set the always-on-top behavior.
    pub fn set_always_on_top(&mut self, _enabled: bool) {
        // Always-on-top is handled by the Desktop Engine (window z-order).
        // This method exists for API completeness.
    }

    /// Get the composite mode.
    pub fn composite_mode(&self) -> OverlayCompositeMode {
        self.composite_mode
    }

    /// Resize the hit-test mask.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.mask.resize(width, height);
    }

    /// Get the mask dimensions.
    pub fn mask_size(&self) -> (u32, u32) {
        (self.mask.width, self.mask.height)
    }

    /// Test if a screen coordinate is clickable (hits Lumi's content).
    pub fn test_hit(&self, x: u32, y: u32) -> bool {
        self.mask.is_opaque(x, y)
    }

    /// Check if readback is pending.
    pub fn readback_pending(&self) -> bool {
        self.readback_pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_test_mask_creation() {
        let mask = HitTestMask::new(1920, 1080);
        assert_eq!(mask.width, 1920);
        assert_eq!(mask.height, 1080);
        assert!(!mask.is_opaque(0, 0));
    }

    #[test]
    fn test_hit_test_mask_set_get() {
        let mut mask = HitTestMask::new(100, 100);
        assert!(!mask.is_opaque(50, 50));

        mask.set_pixel(50, 50, true);
        assert!(mask.is_opaque(50, 50));

        mask.set_pixel(50, 50, false);
        assert!(!mask.is_opaque(50, 50));
    }

    #[test]
    fn test_hit_test_mask_out_of_bounds() {
        let mut mask = HitTestMask::new(100, 100);
        mask.set_pixel(200, 200, true); // Should not panic
        assert!(!mask.is_opaque(200, 200)); // Out of bounds = not opaque
    }

    #[test]
    fn test_hit_test_mask_byte_size() {
        let mask = HitTestMask::new(100, 100);
        // 100 bits per row = 13 bytes per row, 100 rows = 1300 bytes
        assert_eq!(mask.byte_size(), 1300);
    }

    #[test]
    fn test_hit_test_mask_resize() {
        let mut mask = HitTestMask::new(100, 100);
        mask.resize(200, 200);
        assert_eq!(mask.width, 200);
        assert_eq!(mask.height, 200);
    }

    #[test]
    fn test_overlay_renderer_creation() {
        let overlay = OverlayRenderer::new(1920, 1080, OverlayCompositeMode::PreMultiplied);
        assert_eq!(overlay.composite_mode(), OverlayCompositeMode::PreMultiplied);
        assert_eq!(overlay.mask_size(), (1920, 1080));
        assert!(!overlay.is_mask_dirty());
    }

    #[test]
    fn test_overlay_renderer_test_hit() {
        let overlay = OverlayRenderer::new(1920, 1080, OverlayCompositeMode::PreMultiplied);
        // Default mask is all zeros (all transparent).
        assert!(!overlay.test_hit(500, 500));
    }

    #[test]
    fn test_composite_mode_equality() {
        assert_eq!(OverlayCompositeMode::PreMultiplied, OverlayCompositeMode::PreMultiplied);
        assert_ne!(OverlayCompositeMode::PreMultiplied, OverlayCompositeMode::Opaque);
    }
}
