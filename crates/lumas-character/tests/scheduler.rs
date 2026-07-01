//! Tests: BehaviorScheduler delegates to lumas_state::Scheduler, no parallel timer.

use lumas_character::scheduler::BehaviorScheduler;
use lumas_state::error::{EventId, MachineId};
use lumas_state::scheduler::ScheduledEvent;
use tokio::sync::mpsc;
use std::time::Duration;

#[tokio::test]
async fn test_scheduler_creates_one_shot_timer() {
    let (tx, mut rx) = mpsc::unbounded_channel::<ScheduledEvent>();
    let scheduler = BehaviorScheduler::new(tx);

    let timer_id = scheduler.schedule_after(
        MachineId::CHARACTER,
        Duration::from_millis(100),
        EventId::new(1004), // CHAR_IDLE_TIMER_EXPIRED
    );

    // The timer should have been sent to the scheduler channel
    let received = rx.try_recv();
    assert!(received.is_ok(), "Scheduled event should be sent immediately");
    let scheduled = received.unwrap();
    assert_eq!(scheduled.machine_id, MachineId::CHARACTER);
    assert_eq!(scheduled.event.id, EventId::new(1004));
}

#[tokio::test]
async fn test_scheduler_repeating() {
    let (tx, mut rx) = mpsc::unbounded_channel::<ScheduledEvent>();
    let scheduler = BehaviorScheduler::new(tx);

    scheduler.schedule_repeating(
        MachineId::CHARACTER,
        Duration::from_secs(10),
        EventId::new(1005), // CHAR_SLEEP_TIMER_EXPIRED
    );

    let received = rx.try_recv();
    assert!(received.is_ok(), "Repeating event should be sent immediately");
}

#[test]
fn test_behavior_timer_id_format() {
    use lumas_character::scheduler::BehaviorTimerId;
    let id = BehaviorTimerId(42);
    assert_eq!(id.to_string(), "BehaviorTimer(42)");
}
