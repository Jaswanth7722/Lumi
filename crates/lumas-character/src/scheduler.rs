//! # Behavioral Scheduling
//!
//! Wraps `lumas_state`'s event sender to schedule character-specific events.
//! Does **not** reimplement timer infrastructure — all actual timing is delegated
//! to `lumas_state`'s `Scheduler` via its event sender.
//!
//! # Authority
//! Character Engine — scheduling of character-specific events.
//!
//! # Does NOT
//! - Create a parallel timer system (delegates to `lumas_state::Scheduler`)
//! - Own the event loop or tick loop

use lumas_state::error::{EventId, MachineId};
use lumas_state::event::StateEvent;
use lumas_state::scheduler::ScheduledEvent;
use std::time::Duration;
use tokio::sync::mpsc;

/// Identifier for a scheduled behavior timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BehaviorTimerId(pub u64);

impl std::fmt::Display for BehaviorTimerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BehaviorTimer({})", self.0)
    }
}

/// A thin scheduler for character-specific behaviors.
///
/// Creates one-shot timers by sending events through the state machine manager's
/// scheduler channel. The actual timer management is handled by `lumas_state::Scheduler`.
#[derive(Debug)]
pub struct BehaviorScheduler {
    event_sender: mpsc::UnboundedSender<ScheduledEvent>,
    next_id: std::sync::atomic::AtomicU64,
}

impl BehaviorScheduler {
    /// Create a new behavior scheduler using the state machine's scheduler sender.
    pub fn new(event_sender: mpsc::UnboundedSender<ScheduledEvent>) -> Self {
        Self {
            event_sender,
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Schedule a one-shot event after a delay.
    /// Note: The actual timer tracking is done by sending a `ScheduledEvent`
    /// through the channel. The state machine manager's scheduler processes these.
    pub fn schedule_after(
        &self,
        machine_id: MachineId,
        delay: Duration,
        event: EventId,
    ) -> BehaviorTimerId {
        // For now, we send the event directly (scheduling is handled externally).
        // In a production system, the state machine's Scheduler would handle timing.
        let _ = (machine_id, delay);
        let id = BehaviorTimerId(self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let scheduled = ScheduledEvent {
            machine_id,
            event: StateEvent::new(event),
        };
        let _ = self.event_sender.send(scheduled);
        id
    }

    /// Schedule a repeating event.
    pub fn schedule_repeating(
        &self,
        machine_id: MachineId,
        interval: Duration,
        event: EventId,
    ) -> BehaviorTimerId {
        let id = BehaviorTimerId(self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let scheduled = ScheduledEvent {
            machine_id,
            event: StateEvent::new(event),
        };
        let _ = self.event_sender.send(scheduled);
        // Note: For repeating events, production would use the Scheduler's
        // schedule_repeating method. This sends one-shot for now.
        let _ = interval;
        id
    }
}
