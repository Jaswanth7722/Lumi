//! Integration tests for runtime lifecycle state machine.

use lumi_runtime::bootstrap::Bootstrap;
use lumi_runtime::event::RuntimeStarted;
use lumi_runtime::lifecycle::LifecycleManager;

#[tokio::test]
async fn test_bootstrap_succeeds_with_defaults() {
    let mut boot = Bootstrap::new();
    let handle = boot.bootstrap().await;
    assert!(handle.is_ok(), "Bootstrap should succeed with defaults");
}

#[tokio::test]
async fn test_lifecycle_transitions_in_correct_order() {
    let mut lm = LifecycleManager::new();
    assert!(lm.start_bootstrap().is_ok());
    assert!(lm.transition_to_running().is_err()); // Can't skip phases

    // Advance through all bootstrap phases
    for phase in lumi_runtime::lifecycle::BootstrapPhase::ALL {
        assert!(lm.advance_bootstrap(*phase).is_ok());
    }

    assert!(lm.transition_to_running().is_ok());
    assert!(lm.is_running());
}

#[tokio::test]
async fn test_running_state_emits_runtime_started_event() {
    let mut boot = Bootstrap::new();
    let mut rx = boot.event_bus.subscribe::<RuntimeStarted>();

    let _handle = boot.bootstrap().await.unwrap();

    // Should receive RuntimeStarted within 5 seconds
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        rx.recv(),
    )
    .await;

    assert!(result.is_ok(), "Should receive RuntimeStarted event");
    if let Ok(Some(event)) = result {
        assert!(event.version.major >= 0);
    }
}

#[tokio::test]
async fn test_double_start_returns_error() {
    let mut boot = Bootstrap::new();
    let _ = boot.bootstrap().await;

    // Second bootstrap should fail
    let result = boot.bootstrap().await;
    assert!(result.is_err());
}
