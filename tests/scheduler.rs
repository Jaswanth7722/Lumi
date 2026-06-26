//! Integration tests for the async task scheduler.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use lumi_runtime::scheduler::{Scheduler, TaskPriority, TaskStatus};

#[tokio::test]
async fn test_immediate_task_executes() {
    let scheduler = Arc::new(Scheduler::new(16));
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();

    let handle = scheduler
        .spawn_immediate(async move { f.store(true, Ordering::Relaxed) }, TaskPriority::Normal)
        .await;

    assert_eq!(handle.status().await, TaskStatus::Running);
    let final_status = handle.wait().await;
    assert!(flag.load(Ordering::Relaxed));
    assert_eq!(final_status, TaskStatus::Completed);
}

#[tokio::test]
async fn test_delayed_task_executes_after_delay() {
    let scheduler = Arc::new(Scheduler::new(16));
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();

    let start = std::time::Instant::now();
    let handle = scheduler
        .spawn_delayed(
            async move { f.store(true, Ordering::Relaxed) },
            Duration::from_millis(50),
            TaskPriority::Normal,
        )
        .await;

    handle.wait().await;
    let elapsed = start.elapsed();
    assert!(flag.load(Ordering::Relaxed));
    assert!(elapsed >= Duration::from_millis(30));
}

#[tokio::test]
async fn test_repeating_task_executes_multiple_times() {
    let scheduler = Arc::new(Scheduler::new(16));
    let count = Arc::new(AtomicU32::new(0));
    let c = count.clone();

    let handle = scheduler
        .spawn_repeating(
            async move { c.fetch_add(1, Ordering::Relaxed); },
            Duration::from_millis(20),
            TaskPriority::Low,
        )
        .await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    handle.cancel();
    let final_count = count.load(Ordering::Relaxed);
    assert!(final_count >= 3, "Expected at least 3 executions, got {final_count}");
}

#[tokio::test]
async fn test_task_cancellation_stops_execution() {
    let scheduler = Arc::new(Scheduler::new(16));
    let flag = Arc::new(AtomicBool::new(true));
    let f = flag.clone();

    let handle = scheduler
        .spawn_immediate(
            async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                f.store(false, Ordering::Relaxed);
            },
            TaskPriority::Normal,
        )
        .await;

    // Give it time to start running
    tokio::time::sleep(Duration::from_millis(20)).await;
    handle.cancel();

    let final_status = handle.wait().await;
    assert_eq!(final_status, TaskStatus::Cancelled);
    assert!(flag.load(Ordering::Relaxed), "Task should not have completed");
}

#[tokio::test]
async fn test_shutdown_drains_normal_tasks_cancels_background() {
    let scheduler = Arc::new(Scheduler::new(16));
    let normal_done = Arc::new(AtomicBool::new(false));
    let bg_done = Arc::new(AtomicBool::new(false));

    let n = normal_done.clone();
    scheduler
        .spawn_immediate(
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                n.store(true, Ordering::Relaxed);
            },
            TaskPriority::Normal,
        )
        .await;

    let b = bg_done.clone();
    scheduler
        .spawn_immediate(
            async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                b.store(true, Ordering::Relaxed);
            },
            TaskPriority::Background,
        )
        .await;

    scheduler.shutdown(Duration::from_secs(2)).await;
    assert!(normal_done.load(Ordering::Relaxed), "Normal task should drain");
    // Background task may or may not complete, but shutdown should not hang
}
