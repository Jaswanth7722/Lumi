//! # State Identity and Metadata
//!
//! Defines the core `MachineState` trait that all states must satisfy.

use crate::config::StateMachineConfig;
use crate::error::{StateId, StateResult, TimerId};
use std::fmt;
use std::sync::Arc;

/// Implemented by all states (both typestate and runtime).
///
/// This trait provides the minimal interface required for the transition engine
/// to manage state lifecycle. States may be composite (containing substates)
/// or leaf (final atoms of behavior).
///
/// # Thread Safety
/// All state implementations must be `Send + Sync + 'static`.
///
/// # Errors
/// `on_entry` and `on_exit` return `StateError` on failure.
pub trait MachineState: Send + Sync + fmt::Debug + 'static {
    /// Stable identifier — must never change across releases.
    fn id(&self) -> StateId;

    /// Human-readable name for diagnostics and UI.
    fn name(&self) -> &'static str;

    /// Which machine this state belongs to.
    fn machine_id(&self) -> crate::error::MachineId;

    /// Whether this is a composite (parent) state containing substates.
    fn is_composite(&self) -> bool {
        false
    }

    /// Whether this is a final state (no outgoing transitions allowed).
    fn is_final(&self) -> bool {
        false
    }

    /// Maximum time this state is allowed to remain active.
    fn timeout(&self) -> Option<StateTimeout> {
        None
    }

    /// Default entry action, invoked when no transition-specific entry action is defined.
    fn on_entry<'a>(
        &'a self,
        ctx: &'a mut StateContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = StateResult<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    /// Default exit action.
    fn on_exit<'a>(
        &'a self,
        ctx: &'a mut StateContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = StateResult<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

/// Runtime context provided to state lifecycle methods.
///
/// Contains machine identity, timing, history, and a typed extension bag.
///
/// # Concurrency
/// `StateContext` is not `Send` — it is created per-transition and scoped
/// to a single thread during transition execution.
#[derive(Debug)]
pub struct StateContext {
    // Identity
    pub machine_id: crate::error::MachineId,
    pub session_id: crate::error::SessionId,
    pub correlation_id: crate::error::CorrelationId,

    // Current behavioral state
    pub current_state: StateId,
    pub previous_state: Option<StateId>,
    pub state_entered_at: std::time::Instant,
    pub transition_count: u64,

    // Active timers managed by the Scheduler
    pub active_timers: Vec<TimerId>,

    // Metadata injected by subsystems
    pub ai_metadata: AiContextMetadata,
    pub voice_metadata: VoiceContextMetadata,
    pub workspace_metadata: WorkspaceContextMetadata,
    pub desktop_metadata: DesktopContextMetadata,

    // Typed extension bag for subsystem-specific context
    extensions: std::collections::HashMap<std::any::TypeId, Box<dyn std::any::Any + Send + Sync>>,
}

impl StateContext {
    /// Create a new state context.
    pub fn new(machine_id: crate::error::MachineId) -> Self {
        Self {
            machine_id,
            session_id: crate::error::SessionId::new(),
            correlation_id: crate::error::CorrelationId::new(),
            current_state: StateId(0),
            previous_state: None,
            state_entered_at: std::time::Instant::now(),
            transition_count: 0,
            active_timers: Vec::new(),
            ai_metadata: AiContextMetadata::default(),
            voice_metadata: VoiceContextMetadata::default(),
            workspace_metadata: WorkspaceContextMetadata::default(),
            desktop_metadata: DesktopContextMetadata::default(),
            extensions: std::collections::HashMap::new(),
        }
    }

    /// Get subsystem-specific extension data.
    pub fn get<T: std::any::Any + Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    /// Insert or replace subsystem-specific extension data.
    pub fn insert<T: std::any::Any + Send + Sync + 'static>(&mut self, value: T) {
        self.extensions
            .insert(std::any::TypeId::of::<T>(), Box::new(value));
    }

    /// Time spent in current state.
    pub fn time_in_state(&self) -> std::time::Duration {
        self.state_entered_at.elapsed()
    }
}

// ---------------------------------------------------------------------------
// Metadata types
// ---------------------------------------------------------------------------

/// AI subsystem context metadata.
#[derive(Debug, Clone, Default)]
pub struct AiContextMetadata {
    /// Current AI state.
    pub ai_state: Option<String>,
    /// Whether AI is currently inferencing.
    pub is_inferencing: bool,
    /// Whether AI is executing a tool call.
    pub is_tool_execution: bool,
    /// Provider name if known.
    pub provider: Option<String>,
}

/// Voice subsystem context metadata.
#[derive(Debug, Clone, Default)]
pub struct VoiceContextMetadata {
    /// Whether TTS is actively playing.
    pub is_speaking: bool,
    /// Whether listening for wake word.
    pub is_listening: bool,
    /// Whether currently transcribing.
    pub is_transcribing: bool,
}

/// Workspace subsystem context metadata.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceContextMetadata {
    /// Whether an approval dialog is awaiting user input.
    pub has_active_confirmation: bool,
    /// Active panel types.
    pub active_panels: Vec<String>,
}

/// Desktop awareness context metadata.
#[derive(Debug, Clone, Default)]
pub struct DesktopContextMetadata {
    /// Whether focus/DND mode is active.
    pub focus_mode: bool,
    /// Seconds since last user activity.
    pub idle_seconds: u64,
    /// Active window info.
    pub active_window: Option<String>,
}

// ---------------------------------------------------------------------------
// State Timeout
// ---------------------------------------------------------------------------

/// Configuration for automatically timing out of a state.
#[derive(Debug, Clone)]
pub struct StateTimeout {
    /// How long before the timeout fires.
    pub duration: std::time::Duration,
    /// What to do when the timeout fires.
    pub on_timeout: TimeoutAction,
}

/// Action to take when a state timeout fires.
#[derive(Debug, Clone)]
pub enum TimeoutAction {
    /// Fire this event as if it arrived externally.
    Transition(crate::error::EventId),
    /// Bypass guards and force this state.
    ForceState(StateId),
    /// Report to the error handling system.
    EscalateError(String),
    /// Trigger the recovery engine.
    InvokeRecovery,
}

// ---------------------------------------------------------------------------
// State Snapshot
// ---------------------------------------------------------------------------

/// A simple leaf state with no substates and no special behavior.
///
/// Useful for quickly building state machines without defining custom structs.
#[derive(Debug)]
pub struct LeafState {
    id: StateId,
    name: &'static str,
    machine_id: crate::error::MachineId,
    is_final: bool,
    timeout: Option<StateTimeout>,
}

impl LeafState {
    /// Create a new leaf state.
    pub fn new(id: StateId, name: &'static str, machine_id: crate::error::MachineId) -> Self {
        Self {
            id,
            name,
            machine_id,
            is_final: false,
            timeout: None,
        }
    }

    /// Mark this state as final.
    pub fn final_state(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Set a timeout for this state.
    pub fn with_timeout(mut self, timeout: StateTimeout) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl MachineState for LeafState {
    fn id(&self) -> StateId {
        self.id
    }
    fn name(&self) -> &'static str {
        self.name
    }
    fn machine_id(&self) -> crate::error::MachineId {
        self.machine_id
    }
    fn is_final(&self) -> bool {
        self.is_final
    }
    fn timeout(&self) -> Option<StateTimeout> {
        self.timeout.clone()
    }
    fn on_entry<'a>(
        &'a self,
        _ctx: &'a mut StateContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = StateResult<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
    fn on_exit<'a>(
        &'a self,
        _ctx: &'a mut StateContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = StateResult<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

/// A point-in-time view of a machine's state.
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// The machine this snapshot is for.
    pub machine_id: crate::error::MachineId,
    /// Current state ID.
    pub state_id: StateId,
    /// Current state name.
    pub state_name: &'static str,
    /// When the state was entered.
    pub entered_at: std::time::Instant,
    /// Total transitions this machine has performed.
    pub transition_count: u64,
    /// Active substates (for composite states).
    pub active_substates: Vec<StateId>,
}
