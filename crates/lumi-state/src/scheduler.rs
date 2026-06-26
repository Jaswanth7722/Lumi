//! # Delayed Transition Scheduler
//!
//! Manages one-shot and repeating timers that fire events after a delay.
//! Used for idle timeouts, panel auto-dismiss, wake word cooldown, etc.

use crate::error::TimerId;
use crate::event::{EventId, EventPayload, StateEvent};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// A scheduled timer entry.
#[derive(Debug)]
pub struct TimerEntry {
    /// Timer ID.
    pub id: TimerId,
    /// Machine to fire the event on.
    pub machine_id: crate::error::MachineId,
    /// When this timer fires.
    pub fires_at: Instant,
    /// Event ID to fire.
    pub event: EventId,
    /// Event payload.
    pub payload: EventPayload,
    /// Repeating interval (None = one-shot).
    pub repeat: Option<Duration>,
    /// Whether this timer has been cancelled.
    pub cancelled: AtomicBool,
}

/// The scheduler manages delayed and repeating events.
#[derive(Debug)]
pub struct Scheduler {
    /// Active timers.
    timers: DashMap<TimerId, TimerEntry>,
    /// Channel to send scheduled events to the manager.
    event_sender: mpsc::UnboundedSender<ScheduledEvent>,
    /// Next timer ID.
    next_id: AtomicU64,
    /// Resolution of the scheduler tick.
    resolution: Duration,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new(event_sender: mpsc::UnboundedSender<ScheduledEvent>, resolution: Duration) -> Self {
        Self {
            timers: DashMap::new(),
            event_sender,
            next_id: AtomicU64::new(1),
            resolution,
        }
    }

    /// Schedule a one-shot delayed event.
    ///
    /// Returns a `TimerId` that can be used to cancel the timer.
    pub fn schedule_after(
        &self,
        machine_id: crate::error::MachineId,
        delay: Duration,
        event: EventId,
        payload: EventPayload,
    ) -> TimerId {
        let id = TimerId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let entry = TimerEntry {
            id,
            machine_id,
            fires_at: Instant::now() + delay,
            event,
            payload,
            repeat: None,
            cancelled: AtomicBool::new(false),
        };
        self.timers.insert(id, entry);
        id
    }

    /// Schedule a repeating event.
    ///
    /// Must be explicitly cancelled via `cancel()`.
    pub fn schedule_repeating(
        &self,
        machine_id: crate::error::MachineId,
        interval: Duration,
        event: EventId,
    ) -> TimerId {
        let id = TimerId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let entry = TimerEntry {
            id,
            machine_id,
            fires_at: Instant::now() + interval,
            event,
            payload: EventPayload::Empty,
            repeat: Some(interval),
            cancelled: AtomicBool::new(false),
        };
        self.timers.insert(id, entry);
        id
    }

    /// Cancel a timer by ID.
    ///
    /// Returns `true` if the timer was found and cancelled.
    pub fn cancel(&self, timer_id: TimerId) -> bool {
        if let Some(entry) = self.timers.get(&timer_id) {
            entry.cancelled.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Cancel all timers for a specific machine.
    ///
    /// Returns the number of timers cancelled.
    pub fn cancel_all_for_machine(&self, machine_id: crate::error::MachineId) -> usize {
        let mut count = 0;
        for entry in self.timers.iter() {
            if entry.machine_id == machine_id {
                entry.cancelled.store(true, Ordering::Release);
                count += 1;
            }
        }
        count
    }

    /// Run the scheduler loop. This is a background task that checks
    /// for expired timers and sends events.
    pub async fn run(&self) {
        let mut tick = tokio::time::interval(self.resolution);
        loop {
            tick.tick().await;
            self.process_ready_timers().await;
        }
    }

    /// Process all timers that have expired.
    async fn process_ready_timers(&self) {
        let now = Instant::now();
        let ready_ids: Vec<TimerId> = self
            .timers
            .iter()
            .filter(|entry| !entry.cancelled.load(Ordering::Acquire) && entry.fires_at <= now)
            .map(|entry| entry.id)
            .collect();

        for id in ready_ids {
            if let Some(entry) = self.timers.get(&id) {
                if entry.cancelled.load(Ordering::Acquire) {
                    continue;
                }

                let event = StateEvent::new(entry.event)
                    .with_payload(entry.payload.clone())
                    .with_source(crate::event::EventSource::Scheduler {
                        timer_id: entry.id.0,
                    });

                let machine_id = entry.machine_id;
                let _ = self.event_sender.send(ScheduledEvent { machine_id, event });

                // Handle repeating timers
                if let Some(interval) = entry.repeat {
                    entry.fires_at = Instant::now() + interval;
                    entry.cancelled.store(false, Ordering::Release);
                } else {
                    drop(entry);
                    self.timers.remove(&id);
                }
            }
        }
    }

    /// Number of active timers.
    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }
}

/// A scheduled event ready to be processed.
#[derive(Debug)]
pub struct ScheduledEvent {
    /// Target machine.
    pub machine_id: crate::error::MachineId,
    /// The event to fire.
    pub event: StateEvent,
}

// =========================================================================
// Key Scheduled Timers for Lumi
// =========================================================================

/// Default timer definitions for the Lumi platform.
pub mod default_timers {
    use crate::error::MachineId;
    use crate::event::events;
    use std::time::Duration;

    /// Timer definitions: (name, machine, delay, event, description)
    pub const TIMERS: &[(&str, MachineId, Duration, u32, &str)] = &[
        // Idle → Exploring timeout
        (
            "idle_exploring",
            MachineId::CHARACTER,
            Duration::from_secs(300),
            events::CHAR_IDLE_TIMER_EXPIRED.0,
            "5 min idle → exploring",
        ),
        // Idle → Sleeping timeout
        (
            "idle_sleeping",
            MachineId::CHARACTER,
            Duration::from_secs(900),
            events::CHAR_SLEEP_TIMER_EXPIRED.0,
            "15 min idle → sleeping",
        ),
        // Panel auto-dismiss
        (
            "panel_dismiss",
            MachineId::WORKSPACE,
            Duration::from_secs(3),
            events::WS_PANEL_DISMISS_TIMER.0,
            "3 sec post-task panel dismiss",
        ),
        // Wake word cooldown
        (
            "wake_cooldown",
            MachineId::VOICE,
            Duration::from_secs(2),
            events::VOICE_WAKE_COOLDOWN_EXPIRED.0,
            "2 sec wake word cooldown",
        ),
        // Plugin health check
        (
            "plugin_health",
            MachineId::PLUGIN,
            Duration::from_secs(30),
            events::PLUGIN_HEALTH_CHECK.0,
            "30 sec plugin health check",
        ),
    ];
}
