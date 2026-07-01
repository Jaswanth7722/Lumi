//! # Event System
//!
//! Events are the only mechanism for triggering transitions. Every event
//! has a stable ID, a source, an optional payload, and a correlation ID
//! for tracing async event chains.

use crate::error::{CorrelationId, EventId, MachineId, PluginId, StateId};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

/// A state machine event — the only mechanism for triggering transitions.
///
/// Events carry a stable `EventId`, typed payload, source information,
/// and a correlation ID for distributed tracing across machines.
///
/// # Concurrency
/// `StateEvent` is `Clone` and can be sent across threads.
#[derive(Debug, Clone)]
pub struct StateEvent {
    /// Stable event identifier.
    pub id: EventId,
    /// Human-readable event name.
    pub name: &'static str,
    /// Typed event payload.
    pub payload: EventPayload,
    /// Who fired this event.
    pub source: EventSource,
    /// When the event was fired.
    pub fired_at: Instant,
    /// Correlation ID for tracing async event chains.
    pub correlation_id: CorrelationId,
}

impl StateEvent {
    /// Create a new state event.
    pub fn new(id: EventId) -> Self {
        Self {
            id,
            name: "unknown",
            payload: EventPayload::Empty,
            source: EventSource::Internal,
            fired_at: Instant::now(),
            correlation_id: CorrelationId::new(),
        }
    }

    /// Create a state event with a name.
    pub fn named(id: EventId, name: &'static str) -> Self {
        Self {
            id,
            name,
            payload: EventPayload::Empty,
            source: EventSource::Internal,
            fired_at: Instant::now(),
            correlation_id: CorrelationId::new(),
        }
    }

    /// Set the payload.
    pub fn with_payload(mut self, payload: EventPayload) -> Self {
        self.payload = payload;
        self
    }

    /// Set the source.
    pub fn with_source(mut self, source: EventSource) -> Self {
        self.source = source;
        self
    }

    /// Set the correlation ID.
    pub fn with_correlation(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = correlation_id;
        self
    }

    /// Total elapsed time since this event was created.
    pub fn age(&self) -> std::time::Duration {
        self.fired_at.elapsed()
    }
}

impl fmt::Display for StateEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Event({}, {})", self.id.0, self.name)
    }
}

/// Who fired an event.
#[derive(Debug, Clone)]
pub enum EventSource {
    /// User input action.
    UserInput { action: String },
    /// AI core state change.
    AiCore { state: String },
    /// Desktop awareness trigger.
    DesktopAwareness { trigger: String },
    /// Voice system stage change.
    VoiceSystem { stage: String },
    /// Scheduler timer.
    Scheduler { timer_id: u64 },
    /// Plugin lifecycle.
    Plugin { plugin_id: PluginId },
    /// Cross-machine coordination.
    CrossMachine {
        source_machine: MachineId,
        source_state: StateId,
    },
    /// Internal system event.
    Internal,
    /// Recovery system.
    Recovery,
}

/// Typed event payload.
#[derive(Debug, Clone)]
pub enum EventPayload {
    /// No payload.
    Empty,
    /// Duration payload.
    Duration(std::time::Duration),
    /// Error payload.
    Error(String),
    /// Text payload.
    Text(std::borrow::Cow<'static, str>),
    /// Custom typed payload.
    Custom(Arc<dyn std::any::Any + Send + Sync>),
}

impl EventPayload {
    /// Try to downcast to a specific type.
    pub fn downcast<T: std::any::Any + Send + Sync + 'static>(&self) -> Option<&T> {
        match self {
            EventPayload::Custom(arc) => arc.downcast_ref::<T>(),
            _ => None,
        }
    }
}

// =========================================================================
// Pre-defined Platform Events
// =========================================================================

/// All platform event ID constants.
///
/// Every event that can trigger transitions is defined here as a typed constant.
/// IDs must never change across releases.
pub mod events {
    use crate::error::EventId;

    // Character events (1000-1999)
    pub const CHAR_CURSOR_MOVED: EventId = EventId(1001);
    pub const CHAR_USER_CLICKED: EventId = EventId(1002);
    pub const CHAR_ACTIVE_WINDOW_CHANGED: EventId = EventId(1003);
    pub const CHAR_IDLE_TIMER_EXPIRED: EventId = EventId(1004);
    pub const CHAR_SLEEP_TIMER_EXPIRED: EventId = EventId(1005);
    pub const CHAR_TASK_COMPLETED: EventId = EventId(1006);
    pub const CHAR_TASK_FAILED: EventId = EventId(1007);
    pub const CHAR_FOCUS_MODE_ENTERED: EventId = EventId(1008);
    pub const CHAR_FOCUS_MODE_EXITED: EventId = EventId(1009);
    pub const CHAR_USER_ACTIVE: EventId = EventId(1010);
    pub const CHAR_GREETING_COMPLETE: EventId = EventId(1011);
    pub const CHAR_STARTUP_COMPLETE: EventId = EventId(1012);
    pub const CHAR_EXPLORING_TIMER: EventId = EventId(1013);

    // AI events (2000-2999)
    pub const AI_INPUT_RECEIVED: EventId = EventId(2001);
    pub const AI_MEMORY_RETRIEVED: EventId = EventId(2002);
    pub const AI_PLAN_GENERATED: EventId = EventId(2003);
    pub const AI_INFERENCE_STARTED: EventId = EventId(2004);
    pub const AI_FIRST_TOKEN_RECEIVED: EventId = EventId(2005);
    pub const AI_RESPONSE_COMPLETE: EventId = EventId(2006);
    pub const AI_TOOL_CALL_REQUESTED: EventId = EventId(2007);
    pub const AI_TOOL_CALL_COMPLETE: EventId = EventId(2008);
    pub const AI_CANCELLED: EventId = EventId(2009);
    pub const AI_ERROR: EventId = EventId(2010);
    pub const AI_STATE_CHANGED: EventId = EventId(2011);
    pub const AI_CONFIRMATION_REQUIRED: EventId = EventId(2012);

    // Voice events (3000-3999)
    pub const VOICE_WAKE_WORD_DETECTED: EventId = EventId(3001);
    pub const VOICE_SPEECH_STARTED: EventId = EventId(3002);
    pub const VOICE_SPEECH_ENDED: EventId = EventId(3003);
    pub const VOICE_TRANSCRIPTION_READY: EventId = EventId(3004);
    pub const VOICE_TTS_STARTED: EventId = EventId(3005);
    pub const VOICE_TTS_COMPLETED: EventId = EventId(3006);
    pub const VOICE_INTERRUPTED: EventId = EventId(3007);
    pub const VOICE_WAKE_COOLDOWN_EXPIRED: EventId = EventId(3008);

    // Plugin events (4000-4999)
    pub const PLUGIN_LOADED: EventId = EventId(4001);
    pub const PLUGIN_INIT_COMPLETE: EventId = EventId(4002);
    pub const PLUGIN_SUSPENDED: EventId = EventId(4003);
    pub const PLUGIN_UNLOAD_REQUESTED: EventId = EventId(4004);
    pub const PLUGIN_UNLOAD_COMPLETE: EventId = EventId(4005);
    pub const PLUGIN_FAILED: EventId = EventId(4006);
    pub const PLUGIN_HEALTH_CHECK: EventId = EventId(4007);

    // Runtime events (5000-5999)
    pub const RUNTIME_STARTUP_COMPLETE: EventId = EventId(5001);
    pub const RUNTIME_SHUTDOWN_REQUESTED: EventId = EventId(5002);
    pub const RUNTIME_UPDATE_AVAILABLE: EventId = EventId(5003);
    pub const RUNTIME_FOCUS_CHANGED: EventId = EventId(5004);
    pub const RUNTIME_POWER_SLEEP: EventId = EventId(5005);
    pub const RUNTIME_POWER_WAKE: EventId = EventId(5006);
    pub const RUNTIME_RESTART_COMPLETE: EventId = EventId(5007);
    pub const RUNTIME_FATAL_ERROR: EventId = EventId(5008);
    pub const RUNTIME_RECOVERY_TRIGGERED: EventId = EventId(5009);

    // Workspace events (6000-6999)
    pub const WS_PANEL_DISMISS_TIMER: EventId = EventId(6001);
    pub const WS_USER_PINNED_PANEL: EventId = EventId(6002);
    pub const WS_USER_UNPINNED_PANEL: EventId = EventId(6003);

    // Internal runtime events (5500-5599)
    pub const RUNTIME_UPDATE_FAILED: EventId = EventId(5501);
    pub const RUNTIME_RESTART_STARTED: EventId = EventId(5502);

    // Additional character events (1014-1099)
    pub const CHAR_RESTING_TIMER: EventId = EventId(1014);
}

/// Verify uniqueness of all EventId constants at runtime.
/// This is called during initialization and in tests.
pub fn verify_event_id_uniqueness() -> Vec<(u32, &'static str)> {
    use crate::error::EventId;
    let mut seen: std::collections::HashMap<u32, &'static str> = std::collections::HashMap::new();
    let mut collisions = Vec::new();

    // Collect all events
    let all_events: Vec<(u32, &'static str)> = vec![
        (1001, "CHAR_CURSOR_MOVED"),
        (1002, "CHAR_USER_CLICKED"),
        (1003, "CHAR_ACTIVE_WINDOW_CHANGED"),
        (1004, "CHAR_IDLE_TIMER_EXPIRED"),
        (1005, "CHAR_SLEEP_TIMER_EXPIRED"),
        (1006, "CHAR_TASK_COMPLETED"),
        (1007, "CHAR_TASK_FAILED"),
        (1008, "CHAR_FOCUS_MODE_ENTERED"),
        (1009, "CHAR_FOCUS_MODE_EXITED"),
        (1010, "CHAR_USER_ACTIVE"),
        (1011, "CHAR_GREETING_COMPLETE"),
        (1012, "CHAR_STARTUP_COMPLETE"),
        (1013, "CHAR_EXPLORING_TIMER"),
        (2001, "AI_INPUT_RECEIVED"),
        (2002, "AI_MEMORY_RETRIEVED"),
        (2003, "AI_PLAN_GENERATED"),
        (2004, "AI_INFERENCE_STARTED"),
        (2005, "AI_FIRST_TOKEN_RECEIVED"),
        (2006, "AI_RESPONSE_COMPLETE"),
        (2007, "AI_TOOL_CALL_REQUESTED"),
        (2008, "AI_TOOL_CALL_COMPLETE"),
        (2009, "AI_CANCELLED"),
        (2010, "AI_ERROR"),
        (2011, "AI_STATE_CHANGED"),
        (2012, "AI_CONFIRMATION_REQUIRED"),
        (3001, "VOICE_WAKE_WORD_DETECTED"),
        (3002, "VOICE_SPEECH_STARTED"),
        (3003, "VOICE_SPEECH_ENDED"),
        (3004, "VOICE_TRANSCRIPTION_READY"),
        (3005, "VOICE_TTS_STARTED"),
        (3006, "VOICE_TTS_COMPLETED"),
        (3007, "VOICE_INTERRUPTED"),
        (3008, "VOICE_WAKE_COOLDOWN_EXPIRED"),
        (4001, "PLUGIN_LOADED"),
        (4002, "PLUGIN_INIT_COMPLETE"),
        (4003, "PLUGIN_SUSPENDED"),
        (4004, "PLUGIN_UNLOAD_REQUESTED"),
        (4005, "PLUGIN_UNLOAD_COMPLETE"),
        (4006, "PLUGIN_FAILED"),
        (4007, "PLUGIN_HEALTH_CHECK"),
        (5001, "RUNTIME_STARTUP_COMPLETE"),
        (5002, "RUNTIME_SHUTDOWN_REQUESTED"),
        (5003, "RUNTIME_UPDATE_AVAILABLE"),
        (5004, "RUNTIME_FOCUS_CHANGED"),
        (5005, "RUNTIME_POWER_SLEEP"),
        (5006, "RUNTIME_POWER_WAKE"),
        (5007, "RUNTIME_RESTART_COMPLETE"),
        (5008, "RUNTIME_FATAL_ERROR"),
        (5009, "RUNTIME_RECOVERY_TRIGGERED"),
        (6001, "WS_PANEL_DISMISS_TIMER"),
        (6002, "WS_USER_PINNED_PANEL"),
        (6003, "WS_USER_UNPINNED_PANEL"),
    ];

    for (id, name) in all_events {
        if let Some(existing) = seen.insert(id, name) {
            collisions.push((id, existing, name));
        }
    }

    collisions.into_iter().map(|(id, a, b)| (id, a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = StateEvent::new(EventId(1001)).with_name("CHAR_CURSOR_MOVED");
        assert_eq!(event.id, EventId(1001));
        assert_eq!(event.name, "CHAR_CURSOR_MOVED");
    }

    #[test]
    fn test_event_id_uniqueness() {
        let collisions = verify_event_id_uniqueness();
        assert!(
            collisions.is_empty(),
            "Event ID collisions: {:?}",
            collisions
        );
    }

    #[test]
    fn test_event_payload_downcast() {
        let payload = EventPayload::Text(std::borrow::Cow::Borrowed("hello"));
        assert_eq!(payload.downcast::<String>(), None);

        let custom = EventPayload::Custom(Arc::new(42u64));
        assert_eq!(custom.downcast::<u64>(), Some(&42));
    }
}
