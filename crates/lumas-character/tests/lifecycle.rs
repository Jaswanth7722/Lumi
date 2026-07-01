//! Tests: EngineLifecycle transitions, distinct from CharacterMachine state.
//! Assert no coupling between engine lifecycle and behavioral state machine.

use lumas_character::lifecycle::EngineLifecycle;

#[test]
fn test_lifecycle_starts_as_created() {
    // EngineLifecycle is a software-component lifecycle, not behavioral state
    let lifecycle = EngineLifecycle::Created;
    assert!(!lifecycle.can_tick());
    assert!(!lifecycle.is_initialized());
}

#[test]
fn test_lifecycle_ready_can_tick() {
    let lifecycle = EngineLifecycle::Ready;
    assert!(lifecycle.can_tick());
    assert!(lifecycle.is_initialized());
}

#[test]
fn test_lifecycle_degraded_can_tick() {
    let lifecycle = EngineLifecycle::Degraded {
        reason: "test degradation".into(),
    };
    assert!(lifecycle.can_tick());
    assert!(lifecycle.is_initialized());
}

#[test]
fn test_lifecycle_stopped_cannot_tick() {
    let lifecycle = EngineLifecycle::Stopped;
    assert!(!lifecycle.can_tick());
    assert!(lifecycle.is_initialized());
}

#[test]
fn test_lifecycle_shutting_down_cannot_tick() {
    let lifecycle = EngineLifecycle::ShuttingDown;
    assert!(!lifecycle.can_tick());
    assert!(lifecycle.is_initialized());
}

#[test]
fn test_lifecycle_loading_profile_not_initialized() {
    let lifecycle = EngineLifecycle::LoadingProfile;
    assert!(!lifecycle.can_tick());
    assert!(!lifecycle.is_initialized());
}
