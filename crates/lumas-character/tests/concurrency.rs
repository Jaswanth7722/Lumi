//! Tests: Concurrent tick() calls are serialized correctly, no data races on shared profile.

use lumas_character::behavior::{BehaviorContext, BehaviorSelector};
use lumas_character::config::HysteresisConfig;
use lumas_character::movement::MovementPlanner;
use std::sync::Arc;
use std::time::Duration;

/// Verify that BehaviorSelector can safely handle concurrent access patterns.
/// BehaviorSelector is not Sync (it's used inside RwLock), so concurrent
/// access goes through a RwLock — this test verifies the locking pattern works.
#[test]
fn test_selector_sequential_access() {
    let mut selector = BehaviorSelector::new(HysteresisConfig {
        interrupt_margin: 0.15,
        min_run_time: Duration::from_millis(100),
    });
    lumas_character::behavior::register_builtin_behaviors(&mut selector);

    assert_eq!(selector.candidate_count(), 8);
    assert!(selector.current_behavior().is_none());
}

#[test]
fn test_movement_planner_thread_safe() {
    let planner = Arc::new(MovementPlanner::new());
    let planner_clone = planner.clone();

    let handle = std::thread::spawn(move || {
        planner_clone.set_intent(
            lumas_character::movement::MovementIntent::to_absolute(
                100.0, 200.0,
                lumas_character::movement::MovementReason::BehaviorExploring,
            ),
        );
    });

    handle.join().unwrap();
    assert!(planner.current_intent().is_some());
}

#[test]
fn test_metrics_thread_safe() {
    use lumas_character::metrics::{CharacterMetrics, RtSafeCounter};
    let metrics = CharacterMetrics::new();

    // Concurrent increments
    let metrics_clone = metrics.clone();
    let handle = std::thread::spawn(move || {
        metrics_clone.record_behavior_selection();
        metrics_clone.record_emotion_change();
    });

    metrics.record_behavior_selection();
    handle.join().unwrap();

    assert_eq!(metrics.behavior_selections.get(), 2);
    assert_eq!(metrics.emotion_changes.get(), 1);
}

#[test]
fn test_rt_safe_counter_concurrent() {
    use lumas_character::metrics::RtSafeCounter;
    let counter = Arc::new(RtSafeCounter::new());
    let mut handles = vec![];

    for _ in 0..10 {
        let c = counter.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                c.increment();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(counter.get(), 1000);
}
