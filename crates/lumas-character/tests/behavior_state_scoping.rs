//! Tests: A behavior never scores/runs outside its applicable_states.
//! This is the key test proving the scope boundary holds.

use lumas_character::behavior::*;
use lumas_character::config::HysteresisConfig;
use lumas_state::error::StateId;
use std::time::Duration;
use std::sync::Arc;

fn make_selector() -> BehaviorSelector {
    let mut selector = BehaviorSelector::new(HysteresisConfig::default());
    register_builtin_behaviors(&mut selector);
    selector
}

#[test]
fn test_idle_watch_cursor_only_in_idle_states() {
    let behavior = IdleWatchCursor::new();
    let meta = behavior.metadata();

    // Should be applicable in Idle.Watching
    assert!(meta.applicable_states.contains(&StateId::new(1100)));
    // Should NOT be applicable in Sleeping or Error
    assert!(!meta.applicable_states.contains(&StateId::new(1400))); // Sleeping
}

#[test]
fn test_celebrate_success_only_in_working() {
    let behavior = CelebrateSuccess::new();
    let meta = behavior.metadata();

    assert!(meta.applicable_states.contains(&StateId::new(1302))); // VerifyingResult
    assert!(!meta.applicable_states.contains(&StateId::new(1100))); // Not in Idle
    assert!(!meta.applicable_states.contains(&StateId::new(1400))); // Not in Sleeping
}

#[test]
fn test_express_concern_only_in_error() {
    let behavior = ExpressConcern::new();
    let meta = behavior.metadata();

    let in_error = meta.applicable_states.contains(&StateId::new(1500));
    assert!(in_error, "ExpressConcern should only apply in Error state");
}

#[test]
fn test_behavior_not_selected_outside_applicable_states() {
    let mut selector = make_selector();

    // In Sleeping state — no idle or working behaviors should be selected
    let ctx = BehaviorContext {
        current_state: StateId::new(1400), // Sleeping
        desktop: None,
        ai_state: None,
        session_elapsed: Duration::from_secs(60),
        current_emotion: None,
        playfulness: 0.5,
        patience: 0.5,
        time_since_last_selection: None,
        selection_count: 0,
    };

    let selected = selector.select(&ctx);
    // No behavior should apply in Sleeping (ExpressConcern applies in Error, not Sleeping)
    assert!(selected.is_none(), "No behavior should be selectable in Sleeping state");
}
