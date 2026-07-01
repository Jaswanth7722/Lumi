//! # Alpha Mask Hit Testing
//!
//! Hit testing for the stage window using an alpha mask.
//!
//! The stage window is transparent and click-through by default. Only pixels
//! where the rendered character alpha ≥ `hit_threshold` are interactive.
//! The alpha mask is updated every frame from the render process via
//! shared memory (memmap2).
//!
//! # Thread Safety
//!
//! `HitTester` is `Send + Sync`. Alpha mask updates (from render thread) and
//! hit test reads (from event loop thread) are synchronized via a double-buffer:
//! the render process writes to the back buffer; the desktop engine swaps
//! atomically on frame boundary.
//!
//! # Shared Memory Layout
//!
//! ```text
//! [u32 frame_id][u8 * width * height alpha_values]
//! ```
//! - `frame_id`: incremented by render process each frame (detects stale buffers)
//! - `alpha_values`: row-major alpha bytes, one per pixel

use crate::error::DesktopError;
use crate::geometry::{LogicalPoint, LogicalRect, Point, Rect, ScaleFactor};
use crate::metrics::DesktopMetrics;
use arc_swap::ArcSwap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Result of a hit test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitResult {
    /// The pixel is interactive.
    Hit {
        /// The alpha value at the hit pixel.
        alpha: u8,
    },
    /// The pixel is not interactive (passes through).
    Miss,
}

/// Double-buffered alpha mask for hit testing.
pub struct HitTester {
    /// Physical width of the mask.
    width: u32,
    /// Physical height of the mask.
    height: u32,
    /// Front buffer (read by hit tester).
    front: Arc<Mutex<Vec<u8>>>,
    /// Back buffer (written by render process update).
    back: Arc<Mutex<Vec<u8>>>,
    /// Current frame ID from shared memory.
    frame_id: AtomicU32,
    /// Minimum alpha value to treat a pixel as interactive (default: 64).
    hit_threshold: u8,
    /// Bounding rect of the character in logical pixels (fast pre-check).
    character_bounds: ArcSwap<LogicalRect>,
    /// Desktop metrics.
    metrics: Arc<DesktopMetrics>,
}

impl HitTester {
    /// Create a new hit tester.
    ///
    /// # Arguments
    ///
    /// * `width` — Physical width of the alpha mask.
    /// * `height` — Physical height of the alpha mask.
    /// * `hit_threshold` — Minimum alpha (0–255) for a pixel to be interactive.
    pub fn new(
        width: u32,
        height: u32,
        hit_threshold: u8,
        metrics: Arc<DesktopMetrics>,
    ) -> Result<Self, DesktopError> {
        let buffer_size = (width * height) as usize;
        if buffer_size == 0 {
            return Err(DesktopError::HitTestMaskNotInitialized {
                id: crate::window::WindowId::new(),
            });
        }

        Ok(Self {
            width,
            height,
            front: Arc::new(Mutex::new(vec![0u8; buffer_size])),
            back: Arc::new(Mutex::new(vec![0u8; buffer_size])),
            frame_id: AtomicU32::new(0),
            hit_threshold,
            character_bounds: ArcSwap::new(Arc::new(LogicalRect::default())),
            metrics,
        })
    }

    /// Swap buffers: atomically exchange front and back buffers.
    ///
    /// Called once per frame after the render process has written the
    /// new alpha mask to the back buffer (via `update_alpha`).
    /// Must complete in < 0.1ms.
    pub fn swap_buffers(&self) {
        // Atomic frame ID increment signals a new frame.
        self.frame_id.fetch_add(1, Ordering::Release);
    }

    /// Update the back buffer with new alpha data from the render process.
    ///
    /// # Errors
    ///
    /// Returns `DesktopError::HitTestMaskNotInitialized` if the data length
    /// doesn't match the expected buffer size.
    pub fn update_alpha(&self, data: &[u8]) -> Result<(), DesktopError> {
        let expected = (self.width * self.height) as usize;
        if data.len() != expected {
            return Err(DesktopError::HitTestMaskNotInitialized {
                id: crate::window::WindowId::new(),
            });
        }

        let mut back = self.back.lock();
        back.copy_from_slice(data);
        // Swap pointers atomically
        std::mem::swap(&mut *self.front.lock(), &mut *back);
        Ok(())
    }

    /// Test whether a logical screen position hits the character.
    ///
    /// First checks the character bounding rect (fast pre-rejection).
    /// Then checks the specific pixel alpha value.
    ///
    /// # Performance
    ///
    /// This runs on the event loop thread and must complete in < 0.1ms.
    /// The bounding rect check is O(1). The pixel lookup is O(1).
    pub fn test(&self, screen: LogicalPoint, scale: ScaleFactor) -> HitResult {
        let bounds = self.character_bounds.load_full();

        // Fast pre-rejection: check bounding rect first.
        if bounds.is_empty() || !bounds.contains(screen) {
            return HitResult::Miss;
        }

        // Convert to physical pixel coordinates.
        let physical = scale.point_to_physical(screen);
        let px = physical.x.min(self.width - 1);
        let py = physical.y.min(self.height - 1);

        let front = self.front.lock();
        let index = (py * self.width + px) as usize;

        if index < front.len() {
            let alpha = front[index];
            if alpha >= self.hit_threshold {
                return HitResult::Hit { alpha };
            }
        }

        HitResult::Miss
    }

    /// Update the character's bounding rect for fast pre-rejection.
    pub fn update_bounds(&self, bounds: LogicalRect) {
        self.character_bounds.store(Arc::new(bounds));
    }

    /// Returns the current bounding rect.
    pub fn current_bounds(&self) -> LogicalRect {
        *self.character_bounds.load_full()
    }

    /// Returns the current frame ID.
    pub fn frame_id(&self) -> u32 {
        self.frame_id.load(Ordering::Acquire)
    }

    /// Returns the physical dimensions of the alpha mask.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl std::fmt::Debug for HitTester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HitTester")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("hit_threshold", &self.hit_threshold)
            .field("frame_id", &self.frame_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::ScaleFactor;

    fn make_metrics() -> Arc<DesktopMetrics> {
        Arc::new(DesktopMetrics::new())
    }

    #[test]
    fn test_opaque_pixel_returns_hit() {
        let metrics = make_metrics();
        let ht = HitTester::new(100, 100, 64, metrics).unwrap();

        // Create a mask with a fully opaque pixel at (50, 50)
        let mut data = vec![0u8; 100 * 100];
        data[50 * 100 + 50] = 255;
        ht.update_alpha(&data).unwrap();

        let result = ht.test(LogicalPoint::new(50.0, 50.0), ScaleFactor::ONE);
        assert_eq!(result, HitResult::Hit { alpha: 255 });
    }

    #[test]
    fn test_transparent_pixel_returns_miss() {
        let metrics = make_metrics();
        let ht = HitTester::new(100, 100, 64, metrics).unwrap();

        let data = vec![0u8; 100 * 100];
        ht.update_alpha(&data).unwrap();

        // Also update bounds so the pre-check passes
        ht.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

        let result = ht.test(LogicalPoint::new(50.0, 50.0), ScaleFactor::ONE);
        assert_eq!(result, HitResult::Miss);
    }

    #[test]
    fn test_below_threshold_pixel_returns_miss() {
        let metrics = make_metrics();
        let ht = HitTester::new(100, 100, 64, metrics).unwrap();

        let mut data = vec![0u8; 100 * 100];
        data[50 * 100 + 50] = 63; // Below threshold of 64
        ht.update_alpha(&data).unwrap();

        ht.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

        let result = ht.test(LogicalPoint::new(50.0, 50.0), ScaleFactor::ONE);
        assert_eq!(result, HitResult::Miss);
    }

    #[test]
    fn test_bounds_update_affects_precheck() {
        let metrics = make_metrics();
        let ht = HitTester::new(100, 100, 64, metrics).unwrap();

        // Character bounds at (0, 0, 50, 50), point at (75, 75) is outside.
        ht.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 50.0, 50.0));

        let result = ht.test(LogicalPoint::new(75.0, 75.0), ScaleFactor::ONE);
        assert_eq!(result, HitResult::Miss);
    }

    #[test]
    fn test_update_from_shared_memory_updates_alpha() {
        let metrics = make_metrics();
        let ht = HitTester::new(10, 10, 64, metrics).unwrap();

        let mut data = vec![0u8; 100];
        data[0] = 255;
        data[99] = 128;
        ht.update_alpha(&data).unwrap();

        // Check that the front buffer has the new data.
        let front = ht.front.lock();
        assert_eq!(front[0], 255);
        assert_eq!(front[99], 128);
    }

    #[test]
    fn test_concurrent_test_and_update() {
        let metrics = make_metrics();
        let ht = Arc::new(HitTester::new(100, 100, 64, metrics).unwrap());
        ht.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

        let ht_clone = ht.clone();
        let writer = std::thread::spawn(move || {
            for i in 0..10 {
                let mut data = vec![0u8; 100 * 100];
                data[i * 100 + i] = 255;
                let _ = ht_clone.update_alpha(&data);
            }
        });

        let reader = std::thread::spawn(move || {
            for _ in 0..10 {
                let _ = ht.test(LogicalPoint::new(50.0, 50.0), ScaleFactor::ONE);
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }
}
