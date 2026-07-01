//! Tests for geometry operations: rectangles, points, scale factor conversions.

use lumas_desktop::geometry::{
    LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, Point, Rect, ScaleFactor,
    Size,
};

#[test]
fn test_rect_contains_interior_point() {
    let rect = LogicalRect {
        origin: LogicalPoint { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    assert!(rect.contains(LogicalPoint { x: 50.0, y: 50.0 }));
    assert!(rect.contains(LogicalPoint { x: 0.0, y: 0.0 }));
    assert!(rect.contains(LogicalPoint { x: 99.9, y: 99.9 }));
}

#[test]
fn test_rect_does_not_contain_exterior_point() {
    let rect = LogicalRect {
        origin: LogicalPoint { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    assert!(!rect.contains(LogicalPoint { x: -1.0, y: 50.0 }));
    assert!(!rect.contains(LogicalPoint { x: 50.0, y: -1.0 }));
    assert!(!rect.contains(LogicalPoint { x: 101.0, y: 50.0 }));
    assert!(!rect.contains(LogicalPoint { x: 50.0, y: 101.0 }));
}

#[test]
fn test_rect_intersection_returns_correct_overlap() {
    let a = LogicalRect {
        origin: LogicalPoint { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    let b = LogicalRect {
        origin: LogicalPoint { x: 50.0, y: 50.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    let intersection = a.intersection(&b).unwrap();
    assert!((intersection.origin.x - 50.0).abs() < f64::EPSILON);
    assert!((intersection.origin.y - 50.0).abs() < f64::EPSILON);
    assert!((intersection.size.width - 50.0).abs() < f64::EPSILON);
    assert!((intersection.size.height - 50.0).abs() < f64::EPSILON);
}

#[test]
fn test_rect_edge_distance_is_zero_inside() {
    let rect = LogicalRect {
        origin: LogicalPoint { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    let distance = rect.edge_distance(LogicalPoint { x: 50.0, y: 50.0 });
    assert!(distance.abs() < f64::EPSILON);
}

#[test]
fn test_scale_factor_logical_to_physical_roundtrip() {
    let scale = ScaleFactor(2.0);
    let logical: f64 = 100.0;
    let physical: f64 = scale.logical_to_physical(logical);
    let back: f64 = scale.physical_to_logical(physical);
    assert!((logical - back).abs() < f64::EPSILON);
}

#[test]
fn test_logical_to_physical_size_correct() {
    let scale = ScaleFactor(2.0);
    let logical: f64 = 100.0;
    let physical: f64 = scale.logical_to_physical(logical);
    assert!((physical - 200.0).abs() < f64::EPSILON);
}

#[test]
fn test_rect_union() {
    let a = LogicalRect {
        origin: LogicalPoint { x: 0.0, y: 0.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    let b = LogicalRect {
        origin: LogicalPoint { x: 50.0, y: 50.0 },
        size: LogicalSize {
            width: 100.0,
            height: 100.0,
        },
    };
    let union = a.union(&b);
    assert!((union.origin.x - 0.0).abs() < f64::EPSILON);
    assert!((union.origin.y - 0.0).abs() < f64::EPSILON);
    assert!((union.size.width - 150.0).abs() < f64::EPSILON);
    assert!((union.size.height - 150.0).abs() < f64::EPSILON);
}

#[test]
fn test_rect_center() {
    let rect = LogicalRect {
        origin: LogicalPoint { x: 10.0, y: 20.0 },
        size: LogicalSize {
            width: 100.0,
            height: 200.0,
        },
    };
    let center = rect.center();
    assert!((center.x - 60.0).abs() < f64::EPSILON);
    assert!((center.y - 120.0).abs() < f64::EPSILON);
}
