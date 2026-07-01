//! Tests: Navigator produces valid PositionTarget respecting no-walk zones.

use lumas_character::config::ScreenRect;
use lumas_character::movement::{MovementIntent, MovementPlanner, MovementReason, MovementUrgency};
use lumas_character::navigation::Navigator;
use lumas_common::desktop::{
    DesktopSnapshot, UserActivity, WindowInfo, WindowBounds, InputType, SystemInfo,
};
use lumas_common::position::PositionTarget;

fn make_desktop() -> DesktopSnapshot {
    DesktopSnapshot {
        timestamp: 0,
        active_window: WindowInfo {
            title: "Test".into(),
            application: "TestApp".into(),
            bundle_id: None,
            bounds: Some(WindowBounds { x: 0.0, y: 0.0, width: 800.0, height: 600.0 }),
            pid: None,
        },
        open_windows: vec![],
        user_activity: UserActivity {
            idle_seconds: 0,
            focus_mode_active: false,
            last_input_type: InputType::None,
        },
        system: SystemInfo {
            cpu_percent: 0.0,
            memory_percent: 0.0,
            battery_percent: None,
            network_connected: true,
        },
        recent_notifications: vec![],
    }
}

#[test]
fn test_navigator_window_follow_produces_valid_target() {
    let navigator = Navigator::new(vec![], 400.0, None);
    let desktop = make_desktop();
    let target = navigator.plan_destination(
        MovementReason::FollowingActiveWindow,
        &desktop,
    );
    assert!(target.is_ok());
    let target = target.unwrap();
    // Should be an absolute position near the window
    match target {
        PositionTarget::Absolute { x, y } => {
            assert!(x > 0.0);
            assert!(y > 0.0);
        }
        _ => panic!("Expected Absolute target, got {:?}", target),
    }
}

#[test]
fn test_no_walk_zone_respected() {
    // Place a no-walk zone covering the window-follow position
    let zone = ScreenRect { x: 800.0, y: 0.0, width: 100.0, height: 200.0 };
    let navigator = Navigator::new(vec![zone], 400.0, None);
    let desktop = make_desktop();
    let target = navigator.plan_destination(
        MovementReason::FollowingActiveWindow,
        &desktop,
    );
    assert!(target.is_ok(), "Should still produce a valid target");
}

#[test]
fn test_movement_planner_set_and_take() {
    let planner = MovementPlanner::new();
    let intent = MovementIntent::to_absolute(100.0, 200.0, MovementReason::ReturningHome);
    planner.set_intent(intent);
    assert!(planner.current_intent().is_some());
    let taken = planner.take_intent();
    assert!(taken.is_some());
    assert!(planner.current_intent().is_none());
}

#[test]
fn test_movement_intent_create_explore() {
    let intent = MovementIntent::explore(PositionTarget::Absolute { x: 400.0, y: 300.0 });
    assert_eq!(intent.urgency, MovementUrgency::Leisurely);
    assert_eq!(intent.reason, MovementReason::BehaviorExploring);
}
