//! Tests for alpha-mask-based hit testing.

use lumas_desktop::geometry::{LogicalPoint, LogicalRect, LogicalSize, ScaleFactor};
use lumas_desktop::hit_test::{HitResult, HitTester};
use lumas_desktop::metrics::DesktopMetrics;
use std::sync::Arc;

fn make_tester(width: u32, height: u32, threshold: u8) -> (HitTester, Arc<DesktopMetrics>) {
    let metrics = Arc::new(DesktopMetrics::new());
    let tester = HitTester::new(width, height, threshold, metrics.clone()).unwrap();
    (tester, metrics)
}

fn make_mask(width: u32, height: u32, alpha_value: u8) -> Vec<u8> {
    vec![alpha_value; (width * height) as usize]
}

#[test]
fn test_opaque_pixel_returns_hit() {
    let (tester, _metrics) = make_tester(100, 100, 64);
    tester.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

    let mask = make_mask(100, 100, 255);
    tester.update_alpha(&mask).unwrap();

    let result = tester.test(
        LogicalPoint { x: 50.0, y: 50.0 },
        ScaleFactor(1.0),
    );

    assert_eq!(result, HitResult::Hit { alpha: 255 });
}

#[test]
fn test_transparent_pixel_returns_miss() {
    let (tester, _metrics) = make_tester(100, 100, 64);
    tester.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

    let mask = make_mask(100, 100, 0);
    tester.update_alpha(&mask).unwrap();

    let result = tester.test(
        LogicalPoint { x: 50.0, y: 50.0 },
        ScaleFactor(1.0),
    );

    assert_eq!(result, HitResult::Miss);
}

#[test]
fn test_below_threshold_pixel_returns_miss() {
    let (tester, _metrics) = make_tester(100, 100, 128);
    tester.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

    let mut mask = make_mask(100, 100, 0);
    mask[50 * 100 + 50] = 100; // Below threshold of 128
    tester.update_alpha(&mask).unwrap();

    let result = tester.test(
        LogicalPoint { x: 50.0, y: 50.0 },
        ScaleFactor(1.0),
    );

    assert_eq!(result, HitResult::Miss);
}

#[test]
fn test_bounds_update_affects_precheck() {
    let (tester, _metrics) = make_tester(100, 100, 64);

    // Set character bounds to a small region.
    tester.update_bounds(LogicalRect::from_xywh(10.0, 10.0, 80.0, 80.0));

    // Point outside bounds should miss regardless of alpha.
    let result = tester.test(
        LogicalPoint { x: 200.0, y: 200.0 },
        ScaleFactor(1.0),
    );

    assert_eq!(result, HitResult::Miss);
}

#[test]
fn test_alpha_value_at_threshold_returns_hit() {
    let (tester, _metrics) = make_tester(100, 100, 64);
    tester.update_bounds(LogicalRect::from_xywh(0.0, 0.0, 100.0, 100.0));

    let mask = make_mask(100, 100, 64); // Exactly at threshold
    tester.update_alpha(&mask).unwrap();

    let result = tester.test(
        LogicalPoint { x: 50.0, y: 50.0 },
        ScaleFactor(1.0),
    );

    assert_eq!(result, HitResult::Hit { alpha: 64 });
}

#[test]
fn test_update_bounds_returns_correct_bounds() {
    let (tester, _metrics) = make_tester(100, 100, 64);
    assert!(tester.current_bounds().is_empty()); // Default bounds are empty

    tester.update_bounds(LogicalRect::from_xywh(10.0, 20.0, 200.0, 300.0));
    let bounds = tester.current_bounds();
    assert!((bounds.origin.x - 10.0).abs() < 0.001);
    assert!((bounds.origin.y - 20.0).abs() < 0.001);
    assert!((bounds.size.width - 200.0).abs() < 0.001);
    assert!((bounds.size.height - 300.0).abs() < 0.001);
}
