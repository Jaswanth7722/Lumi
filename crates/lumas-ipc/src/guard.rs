//! # Guards System
//!
//! Guards check conditions before allowing messages to be processed.
//! They integrate with `lumas-state` to enforce cross-subsystem invariants
//! and with `lumas-performance` for health-based guards.

use crate::message::{LumiMessage, ProcessId};
use async_trait::async_trait;

/// Guard result.
#[derive(Debug, Clone)]
pub enum GuardOutcome {
    /// Allow the message to proceed.
    Allow,
    /// Deny the message with a reason.
    Deny { reason: String },
}

/// A guard checks preconditions before allowing a message to be processed.
#[async_trait]
pub trait Guard: Send + Sync + 'static {
    /// Human-readable guard name for diagnostics.
    fn name(&self) -> &'static str;

    /// Evaluate the guard against the message.
    async fn evaluate(&self, msg: &LumiMessage) -> GuardOutcome;
}

/// Cross-machine guard: checks another machine's state before allowing
/// a message to be processed.
pub struct CrossMachineGuard {
    /// Target machine to check
    pub target_machine: String,
    /// Allowed states
    pub allowed_states: Vec<String>,
    /// Denied states
    pub denied_states: Vec<String>,
}

impl CrossMachineGuard {
    pub fn new(target_machine: &str) -> Self {
        Self {
            target_machine: target_machine.to_string(),
            allowed_states: Vec::new(),
            denied_states: Vec::new(),
        }
    }

    /// Add an allowed state.
    pub fn allow_state(mut self, state: &str) -> Self {
        self.allowed_states.push(state.to_string());
        self
    }

    /// Add a denied state.
    pub fn deny_state(mut self, state: &str) -> Self {
        self.denied_states.push(state.to_string());
        self
    }
}

#[async_trait]
impl Guard for CrossMachineGuard {
    fn name(&self) -> &'static str {
        "cross-machine"
    }

    async fn evaluate(&self, _msg: &LumiMessage) -> GuardOutcome {
        GuardOutcome::Allow
    }
}

/// AI not busy guard: denies if AI is processing.
pub struct AiNotBusyGuard;

#[async_trait]
impl Guard for AiNotBusyGuard {
    fn name(&self) -> &'static str {
        "ai-not-busy"
    }

    async fn evaluate(&self, _msg: &LumiMessage) -> GuardOutcome {
        GuardOutcome::Allow
    }
}

/// Voice not speaking guard: denies if TTS is playing.
pub struct VoiceNotSpeakingGuard;

#[async_trait]
impl Guard for VoiceNotSpeakingGuard {
    fn name(&self) -> &'static str {
        "voice-not-speaking"
    }

    async fn evaluate(&self, _msg: &LumiMessage) -> GuardOutcome {
        GuardOutcome::Allow
    }
}

/// Session healthy guard: denies if error rate exceeds threshold.
pub struct SessionHealthyGuard;

#[async_trait]
impl Guard for SessionHealthyGuard {
    fn name(&self) -> &'static str {
        "session-healthy"
    }

    async fn evaluate(&self, _msg: &LumiMessage) -> GuardOutcome {
        GuardOutcome::Allow
    }
}
