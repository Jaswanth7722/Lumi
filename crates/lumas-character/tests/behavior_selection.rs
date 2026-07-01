//! Tests: Behavior selection scoring correctness, hysteresis prevents thrashing,
//! hard-precondition filtering.

use lumas_character::behavior::*;
use lumas_character::behavior::HysteresisConfig;
use lumas_state::error::StateId;
use std::time::Duration;
use std::sync::Arc;

fn idle_watching_ctx() -> BehaviorContext {
    BehaviorContext {
        current_state: StateId::new(1100),
        desktop: None,
        ai_state: None,
        session_elapsed: Duration::from_secs(30),
        current_emotion: None,
        playfulness: 0.6,
        patience: 0.7,
        time_since_last_selection: None,
        selection_count: 0,
    }
}

#[test]
fn test_scoring_idle_watch_cursor() {
    let behavior = IdleWatchCursor::new();
    let ctx = idle_watching_ctx();
    let score = behavior.score(&ctx);
    assert!(score.is_some(), "IdleWatchCursor should be applicable in Watching");
    let score_val = score.unwrap();
    assert!(score_val > 0.0 && score_val <= 1.0, "Score should be in (0,1]");
}

#[test]
fn test_scoring_idle_explore() {
    let behavior = IdleExplore::new();
    let ctx = idle_watching_ctx();
    let score = behavior.score(&ctx);
    assert!(score.is_some(), "IdleExplore should be applicable in Watching");
}

#[test]
fn test_hard_precondition_greet_user() {
    let behavior = GreetUser::new();
    // After greeting window, score should be None
    let late_ctx = BehaviorContext {
        session_elapsed: Duration::from_secs(60),
        ..idle_watching_ctx()
    };
    let score = behavior.score(&late_ctx);
    assert!(score.is_none(), "GreetUser should not be applicable after greeting window");
}

#[test]
fn test_hysteresis_prevents_thrashing() {
    let mut selector = BehaviorSelector::new(HysteresisConfig {
        interrupt_margin: 0.5,
        min_run_time: Duration::from_millis(500),
    });
    register_builtin_behaviors(&mut selector);

    let ctx = idle_watching_ctx();
    let first = selector.select(&ctx);
    assert!(first.is_some(), "Should select initial behavior");

    // Immediate re-select — hysteresis should prevent switching
    let second = selector.select(&ctx);
    assert!(second.is_none(), "Hysteresis should prevent rapid switching");
}

#[test]
fn test_candidate_count() {
    let mut selector = BehaviorSelector::new(HysteresisConfig::default());
    register_builtin_behaviors(&mut selector);
    assert_eq!(selector.candidate_count(), 8, "Should have 8 built-in behaviors");
}
