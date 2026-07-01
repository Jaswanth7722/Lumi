//! Tests for monitor management.

use lumas_desktop::geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, ScaleFactor, Size};
use lumas_desktop::metrics::DesktopMetrics;
use lumas_desktop::monitor::{MonitorEvent, MonitorId, MonitorInfo, MonitorManager};
use std::sync::Arc;

fn make_test_monitor(id: MonitorId, x: u32, y: u32, w: u32, h: u32, primary: bool) -> MonitorInfo {
    MonitorInfo {
        id,
        name: format!("Monitor {}-{}", x, y),
        is_primary: primary,
        physical_rect: PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: Size { width: w, height: h },
        },
        work_area: LogicalRect {
            origin: LogicalPoint { x: 0.0, y: 0.0 },
            size: LogicalSize { width: w as f64, height: h as f64 },
        },
        scale_factor: ScaleFactor(1.0),
        refresh_rate_hz: Some(60.0),
        color_depth: 32,
        connected_at: chrono::Utc::now(),
    }
}

#[test]
fn test_primary_monitor_is_identified() {
    let metrics = Arc::new(DesktopMetrics::new());
    let manager = MonitorManager::new(metrics);

    let primary_id = MonitorId::new();
    let secondary_id = MonitorId::new();

    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        primary_id.clone(), 0, 0, 1920, 1080, true,
    )));
    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        secondary_id.clone(), 1920, 0, 1920, 1080, false,
    )));

    let primary = manager.primary();
    assert!(primary.is_some());
    assert_eq!(primary.unwrap().id, primary_id);
}

#[test]
fn test_containing_returns_correct_monitor_for_point() {
    let metrics = Arc::new(DesktopMetrics::new());
    let manager = MonitorManager::new(metrics);

    let left_id = MonitorId::new();
    let right_id = MonitorId::new();

    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        left_id.clone(), 0, 0, 1920, 1080, true,
    )));
    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        right_id.clone(), 1920, 0, 1920, 1080, false,
    )));

    // Point on left monitor
    let contained = manager.containing(LogicalPoint { x: 500.0, y: 500.0 });
    assert!(contained.is_some());
    assert_eq!(contained.unwrap().id, left_id);

    // Point on right monitor
    let contained = manager.containing(LogicalPoint { x: 2500.0, y: 500.0 });
    assert!(contained.is_some());
    assert_eq!(contained.unwrap().id, right_id);
}

#[test]
fn test_nearest_returns_monitor_when_point_off_screen() {
    let metrics = Arc::new(DesktopMetrics::new());
    let manager = MonitorManager::new(metrics);

    let id = MonitorId::new();
    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        id.clone(), 0, 0, 1920, 1080, true,
    )));

    // Point far to the right of the monitor
    let nearest = manager.nearest(LogicalPoint { x: 10000.0, y: 500.0 });
    assert!(nearest.is_some());
}

#[test]
fn test_virtual_desktop_bounds_covers_all_monitors() {
    let metrics = Arc::new(DesktopMetrics::new());
    let manager = MonitorManager::new(metrics);

    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        MonitorId::new(), 0, 0, 1920, 1080, true,
    )));
    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        MonitorId::new(), 1920, 0, 1920, 1080, false,
    )));

    let bounds = manager.virtual_desktop_bounds();
    assert!((bounds.size.width - 3840.0).abs() < f64::EPSILON);
    assert!((bounds.size.height - 1080.0).abs() < f64::EPSILON);
}

#[test]
fn test_monitor_hot_plug_event_updates_list() {
    let metrics = Arc::new(DesktopMetrics::new());
    let manager = MonitorManager::new(metrics);

    assert!(manager.all().is_empty());

    let id = MonitorId::new();
    manager.on_monitor_event(MonitorEvent::Added(make_test_monitor(
        id.clone(), 0, 0, 1920, 1080, true,
    )));

    assert_eq!(manager.all().len(), 1);

    manager.on_monitor_event(MonitorEvent::Removed(id));
    assert!(manager.all().is_empty());
}
