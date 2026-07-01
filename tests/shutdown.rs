//! Integration tests for graceful shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use lumas_runtime::bootstrap::Bootstrap;
use lumas_runtime::event::{RuntimeStopped, ShutdownInitiated};
use lumas_runtime::scheduler::{Scheduler, TaskPriority};

#[tokio::test]
async fn test_shutdown_emits_shutdown_initiated() {
    let mut boot = Bootstrap::new();
    let mut rx = boot.event_bus.subscribe::<ShutdownInitiated>();
    let handle = boot.bootstrap().await.unwrap();

    handle.shutdown().await;

    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        rx.recv(),
    )
    .await;

    assert!(result.is_ok(), "Should receive ShutdownInitiated event");
}

#[tokio::test]
async fn test_shutdown_emits_runtime_stopped() {
    let mut boot = Bootstrap::new();
    let mut rx = boot.event_bus.subscribe::<RuntimeStopped>();
    let handle = boot.bootstrap().await.unwrap();

    handle.shutdown().await;

    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        rx.recv(),
    )
    .await;

    assert!(result.is_ok(), "Should receive RuntimeStopped event");
    if let Ok(Some(event)) = result {
        assert!(event.uptime_secs > 0);
    }
}

#[tokio::test]
async fn test_shutdown_drains_tasks() {
    let scheduler = Arc::new(Scheduler::new(16));
    let task_done = Arc::new(AtomicBool::new(false));
    let td = task_done.clone();

    scheduler
        .spawn_immediate(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                td.store(true, Ordering::Relaxed);
            },
            TaskPriority::Normal,
        )
        .await;

    scheduler.shutdown(std::time::Duration::from_secs(2)).await;
    assert!(task_done.load(Ordering::Relaxed), "Task should have been drained");
}

#[tokio::test]
async fn test_shutdown_completes_within_timeout() {
    let mut boot = Bootstrap::new();
    let handle = boot.bootstrap().await.unwrap();

    let start = std::time::Instant::now();
    handle.shutdown().await;
    let elapsed = start.elapsed();

    // Should complete well within 10 seconds for a default bootstrap
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "Shutdown took too long: {:?}",
        elapsed
    );
}
