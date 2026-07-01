//! # Coordinate System
//!
//! Lumas handles two coordinate spaces throughout:
//! - **Logical pixels** (DPI-independent, `f64`): used by the UI layer and all public APIs
//! - **Physical pixels** (raw screen pixels, `u32`): used by the render engine and hit testing
//!
//! Conversion between the two is performed via `ScaleFactor`.
//!
//! # Thread Safety
//!
//! All geometry types are `Copy`, `Send`, and `Sync`. They have no heap
//! allocations and are safe to pass across thread boundaries.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

/// A point in 2D space. Unit is determined by context (logical or physical).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point<T = f64> {
    /// X coordinate.
    pub x: T,
    /// Y coordinate.
    pub y: T,
}

impl<T> Point<T> {
    /// Create a new point with the given coordinates.
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: Add<Output = T>> Add for Point<T> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl<T: Sub<Output = T>> Sub for Point<T> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl<T: Mul<Output = T> + Copy> Mul<T> for Point<T> {
    type Output = Self;
    fn mul(self, rhs: T) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl<T: Div<Output = T> + Copy> Div<T> for Point<T> {
    type Output = Self;
    fn div(self, rhs: T) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// A size in 2D space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Size<T = f64> {
    /// Width.
    pub width: T,
    /// Height.
    pub height: T,
}

impl<T> Size<T> {
    /// Create a new size with the given dimensions.
    pub fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Size<f64> {
    /// Returns the area of this size.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Returns `true` if both dimensions are zero or negative.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

impl Size<u32> {
    /// Returns the area of this size.
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Returns `true` if either dimension is zero.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

// ---------------------------------------------------------------------------
// Rect
// ---------------------------------------------------------------------------

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect<T = f64> {
    /// The origin (top-left corner).
    pub origin: Point<T>,
    /// The size.
    pub size: Size<T>,
}

impl<T: Copy + Into<f64>> Rect<T> {
    /// Create a new rectangle with the given origin and size.
    pub fn new(origin: Point<T>, size: Size<T>) -> Self {
        Self { origin, size }
    }

    /// Create a rectangle from individual coordinates.
    pub fn from_xywh(x: T, y: T, w: T, h: T) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }

    /// Returns the minimum x coordinate.
    pub fn min_x(&self) -> f64 {
        self.origin.x.into()
    }

    /// Returns the minimum y coordinate.
    pub fn min_y(&self) -> f64 {
        self.origin.y.into()
    }

    /// Returns the maximum x coordinate.
    pub fn max_x(&self) -> f64 {
        self.origin.x.into() + self.size.width.into()
    }

    /// Returns the maximum y coordinate.
    pub fn max_y(&self) -> f64 {
        self.origin.y.into() + self.size.height.into()
    }
}

impl<T: PartialOrd + Copy + Into<f64>> Rect<T> {
    /// Returns `true` if this rectangle contains the given point.
    pub fn contains(&self, point: Point<T>) -> bool {
        let px: f64 = point.x.into();
        let py: f64 = point.y.into();
        px >= self.min_x() && px <= self.max_x() && py >= self.min_y() && py <= self.max_y()
    }

    /// Returns `true` if this rectangle intersects another rectangle.
    pub fn intersects(&self, other: &Rect<T>) -> bool {
        self.min_x() <= other.max_x()
            && self.max_x() >= other.min_x()
            && self.min_y() <= other.max_y()
            && self.max_y() >= other.min_y()
    }

    /// Returns the intersection of two rectangles, or `None` if they don't overlap.
    pub fn intersection(&self, other: &Rect<T>) -> Option<Rect<f64>> {
        if !self.intersects(other) {
            return None;
        }
        let x = self.min_x().max(other.min_x());
        let y = self.min_y().max(other.min_y());
        let w = self.max_x().min(other.max_x()) - x;
        let h = self.max_y().min(other.max_y()) - y;
        Some(Rect {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        })
    }

    /// Returns the union of two rectangles (smallest rect containing both).
    pub fn union(&self, other: &Rect<T>) -> Rect<f64> {
        let x = self.min_x().min(other.min_x());
        let y = self.min_y().min(other.min_y());
        let w = self.max_x().max(other.max_x()) - x;
        let h = self.max_y().max(other.max_y()) - y;
        Rect {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }
}

impl<T: Copy + Into<f64>> Rect<T> {
    /// Returns the center point of the rectangle.
    pub fn center(&self) -> Point<f64> {
        Point::new(
            self.min_x() + self.size.width.into() / 2.0,
            self.min_y() + self.size.height.into() / 2.0,
        )
    }
}

impl Rect<f64> {
    /// Distance from point to the nearest edge of the rect.
    /// Returns 0.0 if the point is inside the rect.
    pub fn edge_distance(&self, point: Point<f64>) -> f64 {
        if self.contains(point) {
            return 0.0;
        }
        let dx = if point.x < self.min_x() {
            self.min_x() - point.x
        } else if point.x > self.max_x() {
            point.x - self.max_x()
        } else {
            0.0
        };
        let dy = if point.y < self.min_y() {
            self.min_y() - point.y
        } else if point.y > self.max_y() {
            point.y - self.max_y()
        } else {
            0.0
        };
        dx.hypot(dy)
    }
}

// ---------------------------------------------------------------------------
// ScaleFactor
// ---------------------------------------------------------------------------

/// Scale factor from logical to physical pixels.
///
/// A scale of 2.0 means 1 logical pixel = 2 physical pixels (Retina/HiDPI).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScaleFactor(pub f64);

impl ScaleFactor {
    /// A 1:1 scale factor (standard DPI).
    pub const ONE: ScaleFactor = ScaleFactor(1.0);

    /// Create a new scale factor.
    pub fn new(scale: f64) -> Self {
        Self(scale.max(1.0))
    }

    /// Returns the scale factor value.
    pub fn get(&self) -> f64 {
        self.0
    }

    /// Convert a logical value to physical pixels.
    pub fn logical_to_physical<T: Into<f64> + From<f64>>(&self, logical: T) -> T {
        let physical: f64 = logical.into() * self.0;
        From::from(physical)
    }

    /// Convert a physical pixel value to logical pixels.
    pub fn physical_to_logical<T: Into<f64> + From<f64>>(&self, physical: T) -> T {
        let logical: f64 = physical.into() / self.0;
        From::from(logical)
    }

    /// Convert a logical `Size<f64>` to physical `Size<u32>`.
    pub fn size_to_physical(&self, size: Size<f64>) -> Size<u32> {
        Size::new(
            (size.width * self.0).round() as u32,
            (size.height * self.0).round() as u32,
        )
    }

    /// Convert a physical `Size<u32>` to logical `Size<f64>`.
    pub fn size_to_logical(&self, size: Size<u32>) -> Size<f64> {
        Size::new(
            size.width as f64 / self.0,
            size.height as f64 / self.0,
        )
    }

    /// Convert a logical `Point<f64>` to physical `Point<u32>`.
    pub fn point_to_physical(&self, point: Point<f64>) -> Point<u32> {
        Point::new(
            (point.x * self.0).round() as u32,
            (point.y * self.0).round() as u32,
        )
    }

    /// Convert a physical `Point<u32>` to logical `Point<f64>`.
    pub fn point_to_logical(&self, point: Point<u32>) -> Point<f64> {
        Point::new(
            point.x as f64 / self.0,
            point.y as f64 / self.0,
        )
    }

    /// Convert a logical `Rect<f64>` to physical `Rect<u32>`.
    pub fn rect_to_physical(&self, rect: Rect<f64>) -> Rect<u32> {
        Rect::new(
            self.point_to_physical(rect.origin),
            self.size_to_physical(rect.size),
        )
    }
}

impl Default for ScaleFactor {
    fn default() -> Self {
        Self::ONE
    }
}

impl fmt::Display for ScaleFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}x", self.0)
    }
}

// ---------------------------------------------------------------------------
// Type Aliases
// ---------------------------------------------------------------------------

/// Logical pixel coordinates (DPI-independent, what callers use).
pub type LogicalPoint = Point<f64>;
/// Logical pixel size.
pub type LogicalSize = Size<f64>;
/// Logical pixel rectangle.
pub type LogicalRect = Rect<f64>;

/// Physical pixel coordinates (actual screen pixels, what GPU uses).
pub type PhysicalPoint = Point<u32>;
/// Physical pixel size.
pub type PhysicalSize = Size<u32>;
/// Physical pixel rectangle.
pub type PhysicalRect = Rect<u32>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains_interior_point() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        assert!(rect.contains(Point::new(50.0, 50.0)));
    }

    #[test]
    fn test_rect_does_not_contain_exterior_point() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        assert!(!rect.contains(Point::new(150.0, 50.0)));
    }

    #[test]
    fn test_rect_intersection_returns_correct_overlap() {
        let a = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let b = Rect::from_xywh(50.0, 50.0, 100.0, 100.0);
        let intersection = a.intersection(&b).unwrap();
        assert_eq!(intersection.origin.x, 50.0);
        assert_eq!(intersection.origin.y, 50.0);
        assert_eq!(intersection.size.width, 50.0);
        assert_eq!(intersection.size.height, 50.0);
    }

    #[test]
    fn test_rect_edge_distance_is_zero_inside() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        assert_eq!(rect.edge_distance(Point::new(50.0, 50.0)), 0.0);
    }

    #[test]
    fn test_rect_edge_distance_positive_outside() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        let dist = rect.edge_distance(Point::new(200.0, 200.0));
        assert!(dist > 0.0);
    }

    #[test]
    fn test_scale_factor_logical_to_physical() {
        let scale = ScaleFactor(2.0);
        let logical = Point::new(100.0, 200.0);
        let physical = scale.point_to_physical(logical);
        assert_eq!(physical.x, 200);
        assert_eq!(physical.y, 400);
    }

    #[test]
    fn test_logical_to_physical_size_correct() {
        let scale = ScaleFactor(1.5);
        let size = LogicalSize::new(200.0, 100.0);
        let physical = scale.size_to_physical(size);
        assert_eq!(physical.width, 300);
        assert_eq!(physical.height, 150);
    }

    #[test]
    fn test_scale_factor_roundtrip() {
        let scale = ScaleFactor(2.0);
        let logical = LogicalPoint::new(100.0, 200.0);
        let physical = scale.point_to_physical(logical);
        let back = scale.point_to_logical(physical);
        assert!((back.x - 100.0).abs() < 0.001);
        assert!((back.y - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_rect_center() {
        let rect = Rect::from_xywh(0.0, 0.0, 100.0, 200.0);
        let center = rect.center();
        assert!((center.x - 50.0).abs() < 0.001);
        assert!((center.y - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_point_ops() {
        let a = Point::new(10.0, 20.0);
        let b = Point::new(5.0, 7.0);
        assert_eq!(a + b, Point::new(15.0, 27.0));
        assert_eq!(a - b, Point::new(5.0, 13.0));
        assert_eq!(a * 2.0, Point::new(20.0, 40.0));
    }

    #[test]
    fn test_size_empty() {
        assert!(Size::<f64>::new(0.0, 100.0).is_empty());
        assert!(Size::<f64>::new(100.0, -1.0).is_empty());
        assert!(!Size::<f64>::new(100.0, 100.0).is_empty());
    }

    #[test]
    fn test_rect_union() {
        let a = Rect::from_xywh(0.0, 0.0, 50.0, 50.0);
        let b = Rect::from_xywh(100.0, 100.0, 50.0, 50.0);
        let u = a.union(&b);
        assert!((u.min_x() - 0.0).abs() < 0.001);
        assert!((u.max_x() - 150.0).abs() < 0.001);
    }
}
