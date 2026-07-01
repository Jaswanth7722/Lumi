//! # Process Event Types
//!
//! Process-specific event types that implement `lumas_runtime::event::Event`.
//!
//! These events are published on the `EventBus` and can be subscribed to
//! by any subsystem. All events carry timestamps and process identifiers
//! for correlation.
//!
//! # Design
//!
//! Every event implements `lumas_runtime::event::Event`, which requires
//! `Send + Sync + Clone + Debug + 'static` and an `event_type()` method.
//! This allows consumers to subscribe to specific event types.

use chrono::{DateTime, Utc};
use lumas_runtime::event::{Event, EventPriority};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::id::ProcessId;

// ---------------------------------------------------------------------------
// ProcessRegistered
// ---------------------------------------------------------------------------

/// Emitted when a process descriptor is registered with the process manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRegistered {
    /// The registered process ID.
    pub id: ProcessId,
    /// The kind of process (e.g., "internal_service", "child_process").
    pub kind: String,
    /// When the registration occurred.
    pub registered_at: DateTime<Utc>,
}

impl Event for ProcessRegistered {
    fn event_type() -> &'static str {
        "ProcessRegistered"
    }
    fn priority() -> EventPriority {
        EventPriority::Normal
    }
}

// ---------------------------------------------------------------------------
// ProcessStarted
// ---------------------------------------------------------------------------

/// Emitted when a process starts successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStarted {
    /// The started process ID.
    pub id: ProcessId,
    /// OS PID if this is a child process.
    pub os_pid: Option<u32>,
    /// Startup duration in milliseconds.
    pub startup_duration_ms: u64,
    /// When the start completed.
    pub started_at: DateTime<Utc>,
}

impl Event for ProcessStarted {
    fn event_type() -> &'static str {
        "ProcessStarted"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// ---------------------------------------------------------------------------
// ProcessStopped
// ---------------------------------------------------------------------------

/// Emitted when a process stops cleanly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStopped {
    /// The stopped process ID.
    pub id: ProcessId,
    /// Reason for stopping.
    pub reason: String,
    /// Total uptime in seconds.
    pub uptime_secs: u64,
    /// When the stop occurred.
    pub stopped_at: DateTime<Utc>,
}

impl Event for ProcessStopped {
    fn event_type() -> &'static str {
        "ProcessStopped"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// ---------------------------------------------------------------------------
// ProcessCrashed
// ---------------------------------------------------------------------------

/// Emitted when a process crashes unexpectedly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessCrashed {
    /// The crashed process ID.
    pub id: ProcessId,
    /// Optional OS exit code.
    pub exit_code: Option<i32>,
    /// Human-readable reason for the crash.
    pub reason: String,
    /// Whether a restart has been scheduled.
    pub restart_scheduled: bool,
    /// Optional restart delay in milliseconds.
    pub restart_delay_ms: Option<u64>,
    /// When the crash was detected.
    pub crashed_at: DateTime<Utc>,
}

impl Event for ProcessCrashed {
    fn event_type() -> &'static str {
        "ProcessCrashed"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// ---------------------------------------------------------------------------
// ProcessRestarted
// ---------------------------------------------------------------------------

/// Emitted when a process is restarted by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRestarted {
    /// The restarted process ID.
    pub id: ProcessId,
    /// How many times this process has been restarted.
    pub restart_count: u32,
    /// Reason for the restart.
    pub reason: String,
    /// When the restart occurred.
    pub restarted_at: DateTime<Utc>,
}

impl Event for ProcessRestarted {
    fn event_type() -> &'static str {
        "ProcessRestarted"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// ---------------------------------------------------------------------------
// ProcessFailed
// ---------------------------------------------------------------------------

/// Emitted when a process fails permanently (max restarts exceeded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessFailed {
    /// The failed process ID.
    pub id: ProcessId,
    /// The final error message.
    pub final_error: String,
    /// Total number of restart attempts before giving up.
    pub total_restarts: u32,
    /// When the failure occurred.
    pub failed_at: DateTime<Utc>,
}

impl Event for ProcessFailed {
    fn event_type() -> &'static str {
        "ProcessFailed"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// ---------------------------------------------------------------------------
// HeartbeatMissed
// ---------------------------------------------------------------------------

/// Emitted when a heartbeat is missed for a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMissed {
    /// The process that missed a heartbeat.
    pub id: ProcessId,
    /// Milliseconds since the last heartbeat.
    pub elapsed_ms: u64,
    /// Number of consecutive missed heartbeats.
    pub consecutive_count: u32,
    /// When the miss was detected.
    pub detected_at: DateTime<Utc>,
}

impl Event for HeartbeatMissed {
    fn event_type() -> &'static str {
        "HeartbeatMissed"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// ---------------------------------------------------------------------------
// HeartbeatRecovered
// ---------------------------------------------------------------------------

/// Emitted when a process recovers after missing heartbeats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRecovered {
    /// The recovered process ID.
    pub id: ProcessId,
    /// How many heartbeats were missed before recovery.
    pub missed_count: u32,
    /// When the recovery was detected.
    pub recovered_at: DateTime<Utc>,
}

impl Event for HeartbeatRecovered {
    fn event_type() -> &'static str {
        "HeartbeatRecovered"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

// ---------------------------------------------------------------------------
// SupervisorIntervention
// ---------------------------------------------------------------------------

/// Emitted when a supervisor applies a strategy to a child failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorIntervention {
    /// The supervisor that performed the intervention.
    pub supervisor_id: ProcessId,
    /// The strategy applied (e.g., "one_for_one", "one_for_all").
    pub strategy: String,
    /// The affected process IDs.
    pub affected_processes: Vec<ProcessId>,
    /// Reason for the intervention.
    pub reason: String,
    /// When the intervention occurred.
    pub occurred_at: DateTime<Utc>,
}

impl Event for SupervisorIntervention {
    fn event_type() -> &'static str {
        "SupervisorIntervention"
    }
    fn priority() -> EventPriority {
        EventPriority::Critical
    }
}

// ---------------------------------------------------------------------------
// CapabilityViolation
// ---------------------------------------------------------------------------

/// Emitted when a capability violation is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityViolation {
    /// The process that caused the violation.
    pub process_id: ProcessId,
    /// The capability involved.
    pub capability: String,
    /// The type of violation ("unauthorized" or "duplicate").
    pub violation_type: String,
    /// When the violation was detected.
    pub detected_at: DateTime<Utc>,
}

impl Event for CapabilityViolation {
    fn event_type() -> &'static str {
        "CapabilityViolation"
    }
    fn priority() -> EventPriority {
        EventPriority::High
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_names() {
        assert_eq!(ProcessRegistered::event_type(), "ProcessRegistered");
        assert_eq!(ProcessCrashed::event_type(), "ProcessCrashed");
        assert_eq!(HeartbeatMissed::event_type(), "HeartbeatMissed");
        assert_eq!(SupervisorIntervention::event_type(), "SupervisorIntervention");
    }

    #[test]
    fn test_process_started_creation() {
        let event = ProcessStarted {
            id: ProcessId::new("test"),
            os_pid: Some(12345),
            startup_duration_ms: 100,
            started_at: Utc::now(),
        };
        assert_eq!(event.os_pid, Some(12345));
    }
}
