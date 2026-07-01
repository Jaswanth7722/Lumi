//! Viewport management — render area control, scissor rects, DPI scaling.
//!
//! Lumas renders to a transparent desktop window that can be resized and moved.
//! The viewport system handles:
//!
//! - **Logical vs physical pixels**: DPI-aware scaling for crisp rendering
//! - **Scissor rects**: Clipping render output to sub-regions (for hit-test masking)
//! - **Render area**: The region of the surface that receives rendered content
//! - **Safe area insets**: Margins for window decorations, taskbar, etc.
//! - **Resolution scaling**: Render at reduced resolution for performance
//!
//! # Coordinate System
//!
//! - `physical`: Actual pixels on the display (used by wgpu).
//! - `logical`: Application pixels (96 DPI = 1:1 with physical).
//!
//! # Frame Budget
//! Viewport operations are ~0.001ms CPU (pure math, no allocations).

use crate::error::RenderError;

/// DPI scale factor. 1.0 = 96 DPI (standard), 2.0 = 192 DPI (Retina/HiDPI).
pub type DpiScale = f64;

/// Logical viewport dimensions (before DPI scaling).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    pub width: f64,
    pub height: f64,
}

impl LogicalSize {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width: width.max(1.0),
            height: height.max(1.0),
        }
    }

    /// Convert to physical pixel size at the given DPI scale.
    pub fn to_physical(&self, dpi: DpiScale) -> PhysicalSize {
        PhysicalSize::new(
            (self.width * dpi).ceil() as u32,
            (self.height * dpi).ceil() as u32,
        )
    }

    /// Aspect ratio (width / height).
    pub fn aspect_ratio(&self) -> f64 {
        self.width / self.height
    }
}

/// Physical viewport dimensions (actual pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

impl PhysicalSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Convert to logical pixels at the given DPI scale.
    pub fn to_logical(&self, dpi: DpiScale) -> LogicalSize {
        LogicalSize::new(
            self.width as f64 / dpi,
            self.height as f64 / dpi,
        )
    }

    /// Aspect ratio (width / height).
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height.max(1) as f64
    }

    /// Area in pixels.
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

impl From<wgpu::Extent3d> for PhysicalSize {
    fn from(extent: wgpu::Extent3d) -> Self {
        Self::new(extent.width, extent.height)
    }
}

/// Scissor rectangle for clipping render output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScissorRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ScissorRect {
    /// Create a new scissor rect that covers the entire viewport.
    pub fn full(size: PhysicalSize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }

    /// Create a scissor rect from physical pixel coordinates.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Convert to wgpu's extent for `set_scissor_rect`.
    pub fn to_wgpu(&self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }

    /// Calculate the area of the scissor rect.
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Returns `true` if the rect has zero area.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Safe area insets (margins for window decorations, taskbar, etc.).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SafeAreaInsets {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

impl SafeAreaInsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        }
    }

    pub fn new(top: f64, bottom: f64, left: f64, right: f64) -> Self {
        Self {
            top: top.max(0.0),
            bottom: bottom.max(0.0),
            left: left.max(0.0),
            right: right.max(0.0),
        }
    }
}

/// Resolution scaling strategy for performance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolutionScale {
    /// Full resolution (1.0x).
    Full,
    /// Three-quarter resolution (0.75x).
    ThreeQuarter,
    /// Half resolution (0.5x).
    Half,
    /// Quarter resolution (0.25x).
    Quarter,
    /// Custom scale factor (clamped to [0.1, 1.0]).
    Custom(f64),
}

impl ResolutionScale {
    /// Compute the effective scale factor.
    pub fn factor(&self) -> f64 {
        match self {
            ResolutionScale::Full => 1.0,
            ResolutionScale::ThreeQuarter => 0.75,
            ResolutionScale::Half => 0.5,
            ResolutionScale::Quarter => 0.25,
            ResolutionScale::Custom(s) => s.clamp(0.1, 1.0),
        }
    }

    /// Apply the scale to a physical size.
    pub fn apply(&self, size: PhysicalSize) -> PhysicalSize {
        let factor = self.factor();
        PhysicalSize::new(
            (size.width as f64 * factor).ceil() as u32,
            (size.height as f64 * factor).ceil() as u32,
        )
    }
}

/// The viewport system — manages render area, DPI scaling, and scissor rects.
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Logical size (application pixels).
    logical_size: LogicalSize,
    /// Physical size (actual pixels after DPI scaling).
    physical_size: PhysicalSize,
    /// DPI scale factor.
    dpi_scale: DpiScale,
    /// Resolution scale factor (for performance).
    resolution_scale: ResolutionScale,
    /// Current scissor rect.
    scissor: ScissorRect,
    /// Safe area insets.
    safe_area: SafeAreaInsets,
    /// Whether the viewport has been resized since last check.
    dirty: bool,
}

impl Viewport {
    /// Create a new viewport from initial dimensions and DPI.
    pub fn new(width: u32, height: u32, dpi_scale: DpiScale) -> Self {
        let physical = PhysicalSize::new(width, height);
        let logical = physical.to_logical(dpi_scale);
        Self {
            scissor: ScissorRect::full(physical),
            logical_size: logical,
            physical_size: physical,
            dpi_scale,
            resolution_scale: ResolutionScale::Full,
            safe_area: SafeAreaInsets::zero(),
            dirty: true,
        }
    }

    // ── Accessors ──

    /// Logical size (before DPI scaling).
    pub fn logical_size(&self) -> &LogicalSize {
        &self.logical_size
    }

    /// Physical size (actual render target pixels).
    pub fn physical_size(&self) -> &PhysicalSize {
        &self.physical_size
    }

    /// Render resolution (after resolution scaling).
    pub fn render_size(&self) -> PhysicalSize {
        self.resolution_scale.apply(self.physical_size)
    }

    /// DPI scale factor.
    pub fn dpi_scale(&self) -> DpiScale {
        self.dpi_scale
    }

    /// Current scissor rect.
    pub fn scissor(&self) -> &ScissorRect {
        &self.scissor
    }

    /// Current resolution scale.
    pub fn resolution_scale(&self) -> ResolutionScale {
        self.resolution_scale
    }

    /// Safe area insets.
    pub fn safe_area(&self) -> &SafeAreaInsets {
        &self.safe_area
    }

    /// Whether the viewport has changed since the last frame.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    // ── Setters ──

    /// Resize the viewport to new physical dimensions.
    pub fn resize(&mut self, width: u32, height: u32) {
        let physical = PhysicalSize::new(width, height);
        self.physical_size = physical;
        self.logical_size = physical.to_logical(self.dpi_scale);
        self.scissor = ScissorRect::full(physical);
        self.dirty = true;
    }

    /// Update DPI scale (e.g., when the window moves to a different monitor).
    pub fn set_dpi_scale(&mut self, scale: DpiScale) {
        if (self.dpi_scale - scale).abs() > 0.001 {
            self.dpi_scale = scale;
            self.logical_size = self.physical_size.to_logical(scale);
            self.dirty = true;
        }
    }

    /// Set the scissor rect for clipping render output.
    pub fn set_scissor(&mut self, rect: ScissorRect) {
        self.scissor = rect;
    }

    /// Reset the scissor rect to cover the full viewport.
    pub fn reset_scissor(&mut self) {
        self.scissor = ScissorRect::full(self.physical_size);
    }

    /// Set the resolution scale factor.
    pub fn set_resolution_scale(&mut self, scale: ResolutionScale) {
        self.resolution_scale = scale;
        self.dirty = true;
    }

    /// Set safe area insets.
    pub fn set_safe_area(&mut self, insets: SafeAreaInsets) {
        self.safe_area = insets;
        self.dirty = true;
    }

    // ── Utility ──

    /// Compute the render target extent for wgpu (from resolution-scaled size).
    pub fn render_extent(&self) -> wgpu::Extent3d {
        let size = self.render_size();
        wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        }
    }

    /// Create a wgpu `RenderPassColorAttachment` for the full viewport.
    pub fn color_attachment<'a>(
        &self,
        view: &'a wgpu::TextureView,
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPassColorAttachment<'a> {
        wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
                depth_slice: None,
            ops: wgpu::Operations {
                load: load_op,
                store: wgpu::StoreOp::Store,
            },
        }
    }

    /// Create a wgpu `RenderPassDepthStencilAttachment` for the full viewport.
    pub fn depth_attachment<'a>(
        &self,
        view: &'a wgpu::TextureView,
        depth_load: wgpu::LoadOp<f32>,
    ) -> wgpu::RenderPassDepthStencilAttachment<'a> {
        wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }
    }

    /// Compute the orthographic projection bounds for the character camera
    /// that match the viewport's aspect ratio.
    pub fn orthographic_bounds(&self) -> (f32, f32, f32, f32) {
        let aspect = self.physical_size.aspect_ratio() as f32;
        let half_height = 5.0;
        let half_width = half_height * aspect;
        (-half_width, half_width, -half_height, half_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_size_creation() {
        let logical = LogicalSize::new(1920.0, 1080.0);
        assert!((logical.width - 1920.0).abs() < f64::EPSILON);
        assert!((logical.height - 1080.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_logical_size_clamps_to_minimum() {
        let logical = LogicalSize::new(0.0, 0.0);
        assert_eq!(logical.width, 1.0);
        assert_eq!(logical.height, 1.0);
    }

    #[test]
    fn test_physical_size_conversion() {
        let physical = PhysicalSize::new(1920, 1080);
        assert_eq!(physical.width, 1920);
        assert_eq!(physical.height, 1080);
    }

    #[test]
    fn test_dpi_conversion() {
        let logical = LogicalSize::new(1920.0, 1080.0);
        let physical = logical.to_physical(2.0);
        assert_eq!(physical.width, 3840);
        assert_eq!(physical.height, 2160);

        let back = physical.to_logical(2.0);
        assert!((back.width - 1920.0).abs() < 1.0);
    }

    #[test]
    fn test_scissor_rect_full() {
        let size = PhysicalSize::new(1920, 1080);
        let scissor = ScissorRect::full(size);
        assert_eq!(scissor.width, 1920);
        assert_eq!(scissor.height, 1080);
        assert!(!scissor.is_empty());
    }

    #[test]
    fn test_scissor_area() {
        let rect = ScissorRect::new(0, 0, 100, 100);
        assert_eq!(rect.area(), 10000);
    }

    #[test]
    fn test_resolution_scale_factor() {
        assert!((ResolutionScale::Full.factor() - 1.0).abs() < f64::EPSILON);
        assert!((ResolutionScale::Half.factor() - 0.5).abs() < f64::EPSILON);
        assert!((ResolutionScale::Custom(0.3).factor() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_resolution_scale_clamps() {
        assert!((ResolutionScale::Custom(0.05).factor() - 0.1).abs() < f64::EPSILON);
        assert!((ResolutionScale::Custom(2.0).factor() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_resolution_scale_apply() {
        let size = PhysicalSize::new(1920, 1080);
        let half = ResolutionScale::Half.apply(size);
        assert_eq!(half.width, 960);
        assert_eq!(half.height, 540);
    }

    #[test]
    fn test_viewport_creation() {
        let vp = Viewport::new(1920, 1080, 1.0);
        assert_eq!(vp.physical_size().width, 1920);
        assert_eq!(vp.logical_size().width, 1920.0);
        assert!(vp.is_dirty());
    }

    #[test]
    fn test_viewport_resize() {
        let mut vp = Viewport::new(1920, 1080, 1.0);
        vp.clear_dirty();
        vp.resize(800, 600);
        assert_eq!(vp.physical_size().width, 800);
        assert!(vp.is_dirty());
    }

    #[test]
    fn test_viewport_dpi_change() {
        let mut vp = Viewport::new(1920, 1080, 1.0);
        vp.clear_dirty();
        vp.set_dpi_scale(2.0);
        assert!((vp.dpi_scale() - 2.0).abs() < 0.001);
        assert!((vp.logical_size().width - 960.0).abs() < 1.0);
    }

    #[test]
    fn test_render_extent() {
        let vp = Viewport::new(1920, 1080, 1.0);
        let extent = vp.render_extent();
        assert_eq!(extent.width, 1920);
        assert_eq!(extent.height, 1080);
    }

    #[test]
    fn test_orthographic_bounds() {
        let vp = Viewport::new(1920, 1080, 1.0);
        let (left, right, bottom, top) = vp.orthographic_bounds();
        assert!((right - left).abs() - 10.0 * (1920.0 / 1080.0) < 0.001);
        assert!((top - bottom).abs() - 10.0 < 0.001);
    }

    #[test]
    fn test_safe_area() {
        let insets = SafeAreaInsets::new(30.0, 0.0, 0.0, 0.0);
        assert!((insets.top - 30.0).abs() < f64::EPSILON);
        let zero = SafeAreaInsets::zero();
        assert_eq!(zero.top, 0.0);
    }
}
