//! # Restart Engine Integration Tests
//!
//! Tests for restart policies, sliding windows, backoff, and jitter.

use lumas_process::restart::{RestartAction, RestartEngine, RestartPolicy, RestartRecord};
use lumas_process::id::ProcessId;

fn make_id(name: &str) -> ProcessId {
    ProcessId::new(name)
}

#[test]
fn test_immediate_policy_restarts_without_delay() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    let id = make_id("test");

    let action = engine.next_action(
        &id,
        &RestartPolicy::Immediate {
            max_restarts: 3,
            window_secs: 60,
        },
        &mut record,
    );

    match action {
        RestartAction::RestartAfter { delay } => {
            assert_eq!(delay.as_millis(), 0);
        }
        _ => panic!("Expected RestartAfter with zero delay"),
    }
}

#[test]
fn test_exponential_backoff_increases_delay_correctly() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    let id = make_id("test");

    let policy = RestartPolicy::ExponentialBackoff {
        initial_delay_ms: 100,
        multiplier: 2.0,
        max_delay_ms: 30_000,
        max_restarts: 5,
        window_secs: 3600,
        jitter_percent: 0, // No jitter for deterministic test
    };

    // First restart
    if let RestartAction::RestartAfter { delay } = engine.next_action(&id, &policy, &mut record) {
        assert!(delay.as_millis() >= 100);
    } else {
        panic!("Expected RestartAfter");
    }

    // Second restart (should be ~200ms + no jitter)
    if let RestartAction::RestartAfter { delay } = engine.next_action(&id, &policy, &mut record) {
        assert!(delay.as_millis() >= 200 && delay.as_millis() <= 250);
    } else {
        panic!("Expected RestartAfter");
    }
}

#[test]
fn test_max_restarts_exceeded_transitions_to_failed() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    let id = make_id("test");

    let policy = RestartPolicy::Immediate {
        max_restarts: 2,
        window_secs: 3600,
    };

    // First restart — OK
    assert!(matches!(
        engine.next_action(&id, &policy, &mut record),
        RestartAction::RestartAfter { .. }
    ));

    // Second restart — OK
    assert!(matches!(
        engine.next_action(&id, &policy, &mut record),
        RestartAction::RestartAfter { .. }
    ));

    // Third restart — GivingUp
    assert!(matches!(
        engine.next_action(&id, &policy, &mut record),
        RestartAction::GivingUp
    ));
}

#[test]
fn test_restart_window_resets_after_window_expires() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    record.restart_count = 5; // Pretend we had 5 restarts
    record.window_start = std::time::Instant::now()
        - std::time::Duration::from_secs(1); // Window expired

    let id = make_id("test");
    let policy = RestartPolicy::Immediate {
        max_restarts: 3,
        window_secs: 0, // Window is already past
    };

    // Window expired, so restart count should reset
    let action = engine.next_action(&id, &policy, &mut record);
    assert!(matches!(action, RestartAction::RestartAfter { .. }));
    assert_eq!(record.restart_count, 1); // Reset to 1 after recording
}

#[test]
fn test_never_policy_escalates_immediately() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    let id = make_id("test");

    let action = engine.next_action(&id, &RestartPolicy::Never, &mut record);
    assert!(matches!(action, RestartAction::GivingUp));
}

#[test]
fn test_manual_recovery_awaits_operator_command() {
    let engine = RestartEngine::new();
    let mut record = RestartRecord::new();
    let id = make_id("test");

    let action = engine.next_action(&id, &RestartPolicy::ManualRecovery, &mut record);
    assert!(matches!(action, RestartAction::AwaitManual));
}
