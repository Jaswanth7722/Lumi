//! Tests: BlinkScheduler produces randomized intervals within bounds.

use lumas_character::expression::{BlinkScheduler, BlinkState, ExpressionTargets, LookAtTarget, GestureId, compute_expression_targets};
use std::time::Duration;

#[test]
fn test_blink_scheduler_creation() {
    let scheduler = BlinkScheduler::default();
    assert!(!scheduler.suppressed);
    assert!(scheduler.time_until_next_blink().as_millis() > 0);
}

#[test]
fn test_blink_triggers_within_interval_bounds() {
    let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(10));
    let state = scheduler.check_blink();
    assert_eq!(state, BlinkState::Closing);
}

#[test]
fn test_blink_suppression_works() {
    let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
    scheduler.suppress(Duration::from_secs(60));
    std::thread::sleep(Duration::from_millis(10));
    let state = scheduler.check_blink();
    assert_eq!(state, BlinkState::Open);
}

#[test]
fn test_gesture_selection_by_emotion() {
    use lumas_common::emotion::Emotion;
    let mut scheduler = BlinkScheduler::default();

    let thinking = compute_expression_targets(None, &mut scheduler, &Emotion::Thinking, 0.0);
    assert_eq!(thinking.gesture, Some(GestureId::ThinkPose));

    let mut scheduler2 = BlinkScheduler::default();
    let happy = compute_expression_targets(None, &mut scheduler2, &Emotion::Happy, 0.0);
    assert_eq!(happy.gesture, Some(GestureId::Nod));
}

#[test]
fn test_surprise_suppresses_blink() {
    use lumas_common::emotion::Emotion;
    let mut scheduler = BlinkScheduler::new(Duration::from_millis(1), Duration::from_millis(1));
    compute_expression_targets(None, &mut scheduler, &Emotion::Surprised, 0.0);
    let state = scheduler.check_blink();
    assert_eq!(state, BlinkState::Open);
}

#[test]
fn test_look_at_target_creation() {
    let target = LookAtTarget { x: 500.0, y: 300.0, weight: 0.8 };
    assert!((target.weight - 0.8).abs() < f32::EPSILON);
}

#[test]
fn test_blink_interval_can_be_updated() {
    let mut scheduler = BlinkScheduler::new(Duration::from_secs(2), Duration::from_secs(6));
    scheduler.set_interval(Duration::from_millis(500), Duration::from_secs(2));
    assert!(scheduler.time_until_next_blink().as_millis() <= 2000);
}
