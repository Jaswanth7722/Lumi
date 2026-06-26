//! # Error Event Bus Integration
//!
//! Defines error event types emitted to the Lumi event bus.
//! Events are published asynchronously without blocking the error reporting path.
//! If the IPC/event bus is unavailable, events are queued in a bounded buffer.
//!
//! # Thread Safety
//! `ErrorEventEmitter` is `Send + Sync` and can be shared across threads.

use crate::error_code::ErrorCode;
use crate::recovery::{RecoveryOutcome, RecoveryStrategy};
use crate::report::ErrorReport;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

/// Error events emitted to the event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorEvent {
    /// An error occurred.
    ErrorOccurred {
        /// User-safe error report.
        report: ErrorReport,
        /// Correlation ID for tracing.
        correlation_id: Uuid,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
    /// Recovery has started for an error.
    RecoveryStarted {
        /// The error code being recovered from.
        error_code: ErrorCode,
        /// Human-readable strategy name.
        strategy: String,
        /// Which attempt this is (1-based).
        attempt: u32,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
    /// Recovery completed.
    RecoveryCompleted {
        /// The error code that was recovered from.
        error_code: ErrorCode,
        /// The outcome of recovery.
        outcome: RecoveryOutcome,
        /// Duration of recovery in milliseconds.
        duration_ms: u64,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
    /// A crash report was written to disk.
    CrashReportWritten {
        /// UUID of the crash report.
        crash_id: Uuid,
        /// File path of the written report.
        path: PathBuf,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
    /// A failure pattern was detected.
    PatternDetected {
        /// The error code being detected.
        error_code: ErrorCode,
        /// Number of occurrences in the detection window.
        occurrence_count: u32,
        /// The detection window in seconds.
        window_secs: u32,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
}

impl ErrorEvent {
    /// Get the timestamp of this event.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            ErrorEvent::ErrorOccurred { timestamp, .. } => *timestamp,
            ErrorEvent::RecoveryStarted { timestamp, .. } => *timestamp,
            ErrorEvent::RecoveryCompleted { timestamp, .. } => *timestamp,
            ErrorEvent::CrashReportWritten { timestamp, .. } => *timestamp,
            ErrorEvent::PatternDetected { timestamp, .. } => *timestamp,
        }
    }

    /// Get the event type name as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            ErrorEvent::ErrorOccurred { .. } => "error_occurred",
            ErrorEvent::RecoveryStarted { .. } => "recovery_started",
            ErrorEvent::RecoveryCompleted { .. } => "recovery_completed",
            ErrorEvent::CrashReportWritten { .. } => "crash_report_written",
            ErrorEvent::PatternDetected { .. } => "pattern_detected",
        }
    }
}

/// A trait for emitting error events to the event bus.
pub trait ErrorEventBus: Send + Sync {
    /// Publish an error event.
    fn publish(&self, event: ErrorEvent);
}

/// Event emitter that publishes error events to an event bus.
///
/// Events are published asynchronously. If the bus is unavailable,
/// events are queued in a bounded buffer (max 1,000 events).
#[derive(Debug, Clone)]
pub struct ErrorEventEmitter {
    /// The underlying event bus.
    bus: Option<Arc<dyn ErrorEventBus>>,
    /// Pending events queue (bounded to 1,000).
    pending: Arc<parking_lot::Mutex<Vec<ErrorEvent>>>,
    /// Maximum pending events.
    max_pending: usize,
}

impl ErrorEventEmitter {
    /// Create a new event emitter.
    pub fn new() -> Self {
        Self {
            bus: None,
            pending: Arc::new(parking_lot::Mutex::new(Vec::new())),
            max_pending: 1000,
        }
    }

    /// Create an event emitter with a connected bus.
    pub fn with_bus(bus: Arc<dyn ErrorEventBus>) -> Self {
        Self {
            bus: Some(bus),
            pending: Arc::new(parking_lot::Mutex::new(Vec::new())),
            max_pending: 1000,
        }
    }

    /// Set the event bus.
    pub fn set_bus(&mut self, bus: Arc<dyn ErrorEventBus>) {
        let mut pending = self.pending.lock();
        // Replay pending events
        if let Some(bus) = Some(bus.clone()) {
            for event in pending.drain(..) {
                bus.publish(event);
            }
        }
        self.bus = Some(bus);
    }

    /// Emit an error event.
    ///
    /// If the bus is connected, publishes immediately.
    /// Otherwise, queues the event in the pending buffer.
    ///
    /// # Thread Safety
    /// This method is thread-safe and can be called from any thread.
    ///
    /// # Panics
    /// Does not panic.
    pub fn emit(&self, event: ErrorEvent) {
        if let Some(ref bus) = self.bus {
            bus.publish(event);
        } else {
            let mut pending = self.pending.lock();
            if pending.len() < self.max_pending {
                pending.push(event);
            }
            // If buffer is full, drop the event (non-blocking)
        }
    }

    /// Flush all pending events to the bus.
    pub fn flush(&self) {
        if let Some(ref bus) = self.bus {
            let mut pending = self.pending.lock();
            for event in pending.drain(..) {
                bus.publish(event);
            }
        }
    }
}

impl Default for ErrorEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

// Implement Event trait from lumi-runtime if available
// This is a compile-time bridge that doesn't require lumi-runtime to be present

impl std::fmt::Display for ErrorEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ErrorEvent::{} at {}",
            self.event_type(),
            self.timestamp()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::category::ErrorCategory;
    use crate::error::LumiError;
    use crate::error_code::ErrorCode;
    use crate::report::ReportFormat;

    #[derive(Debug, Clone)]
    struct TestBus {
        events: Arc<parking_lot::Mutex<Vec<ErrorEvent>>>,
    }

    impl ErrorEventBus for TestBus {
        fn publish(&self, event: ErrorEvent) {
            self.events.lock().push(event);
        }
    }

    #[test]
    fn test_event_creation() {
        let error = LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test",
        );
        let report = ErrorReport::from_error(&error, ReportFormat::UserFacing);
        let event = ErrorEvent::ErrorOccurred {
            report,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        };
        assert_eq!(event.event_type(), "error_occurred");
    }

    #[test]
    fn test_emitter_with_bus() {
        let bus = Arc::new(TestBus {
            events: Arc::new(parking_lot::Mutex::new(Vec::new())),
        });
        let emitter = ErrorEventEmitter::with_bus(bus.clone());

        let error = LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test",
        );
        let report = ErrorReport::from_error(&error, ReportFormat::UserFacing);
        emitter.emit(ErrorEvent::ErrorOccurred {
            report,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        });

        assert_eq!(bus.events.lock().len(), 1);
    }

    #[test]
    fn test_emitter_pending_queue() {
        let emitter = ErrorEventEmitter::new();

        let error = LumiError::new(
            ErrorCode::AI_INFERENCE_FAILED,
            ErrorCategory::AiCore { provider: None },
            "test",
        );
        let report = ErrorReport::from_error(&error, ReportFormat::UserFacing);

        // Without a bus, events are queued
        emitter.emit(ErrorEvent::ErrorOccurred {
            report,
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        });

        assert_eq!(emitter.pending.lock().len(), 1);
    }

    #[test]
    fn test_recovery_completed_event() {
        let event = ErrorEvent::RecoveryCompleted {
            error_code: ErrorCode::AI_INFERENCE_FAILED,
            outcome: RecoveryOutcome::Recovered,
            duration_ms: 150,
            timestamp: Utc::now(),
        };
        assert_eq!(event.event_type(), "recovery_completed");
    }
}
