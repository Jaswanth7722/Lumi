//! # Guard System
//!
//! Guards are predicates that must pass for a transition to proceed.
//! All guards are async and can fail with `GuardError`.

use crate::context::StateContext;
use crate::error::{GuardError, MachineId, StateId, StateResult};
use crate::event::{EventId, StateEvent};
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

/// A guard evaluates whether a transition is allowed.
///
/// Guards receive an immutable reference to `StateContext` — they must
/// never mutate context. Guards that need to inspect another machine's
/// state should use `CrossMachineGuard`.
///
/// # Errors
/// Guards may return `GuardError` if evaluation itself fails.
#[async_trait]
pub trait Guard: Send + Sync + fmt::Debug + 'static {
    /// Human-readable guard name.
    fn name(&self) -> &'static str;

    /// Evaluate whether the transition should be allowed.
    async fn evaluate(
        &self,
        ctx: &StateContext,
        event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError>;
}

/// Outcome of a guard evaluation.
#[derive(Debug, Clone)]
pub enum GuardOutcome {
    /// Transition is allowed.
    Allow,
    /// Transition is denied with a reason.
    Deny {
        reason: std::borrow::Cow<'static, str>,
    },
}

/// A guard that checks another machine's current state.
///
/// This is the primary mechanism for enforcing cross-subsystem invariants.
///
/// # Example
/// ```ignore
/// CrossMachineGuard::new(
///     MachineId::AI,
///     vec![/* allowed states */],
///     vec![/* denied states */],
///     manager.clone(),
/// )
/// ```
#[derive(Debug)]
pub struct CrossMachineGuard {
    /// The machine to inspect.
    pub target_machine: MachineId,
    /// States that allow the transition.
    pub allowed_states: BTreeSet<StateId>,
    /// States that deny the transition.
    pub denied_states: BTreeSet<StateId>,
    /// Reference to the manager for querying state.
    pub manager: Arc<dyn StateQuery + Send + Sync>,
}

impl CrossMachineGuard {
    /// Create a new cross-machine guard.
    pub fn new(
        target_machine: MachineId,
        allowed_states: Vec<StateId>,
        denied_states: Vec<StateId>,
        manager: Arc<dyn StateQuery + Send + Sync>,
    ) -> Self {
        Self {
            target_machine,
            allowed_states: allowed_states.into_iter().collect(),
            denied_states: denied_states.into_iter().collect(),
            manager,
        }
    }
}

#[async_trait]
impl Guard for CrossMachineGuard {
    fn name(&self) -> &'static str {
        "CrossMachineGuard"
    }

    async fn evaluate(
        &self,
        _ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        let current = self.manager.current_state_for(self.target_machine);
        match current {
            Some(state_id) => {
                if !self.allowed_states.is_empty() && !self.allowed_states.contains(&state_id) {
                    return Ok(GuardOutcome::Deny {
                        reason: format!(
                            "Machine {:?} is in state {}, which is not in allowed set",
                            self.target_machine, state_id
                        )
                        .into(),
                    });
                }
                if self.denied_states.contains(&state_id) {
                    return Ok(GuardOutcome::Deny {
                        reason: format!(
                            "Machine {:?} is in denied state {}",
                            self.target_machine, state_id
                        )
                        .into(),
                    });
                }
                Ok(GuardOutcome::Allow)
            }
            None => Ok(GuardOutcome::Deny {
                reason: format!("Target machine {:?} not found", self.target_machine).into(),
            }),
        }
    }
}

/// Trait for querying machine state (minimal interface for CrossMachineGuard).
pub trait StateQuery {
    /// Get the current state of a machine.
    fn current_state_for(&self, machine_id: MachineId) -> Option<StateId>;
}

// =========================================================================
// Built-in Guards
// =========================================================================

/// Denies transition if focus/DND mode is active.
#[derive(Debug)]
pub struct NotInFocusMode;

#[async_trait]
impl Guard for NotInFocusMode {
    fn name(&self) -> &'static str {
        "NotInFocusMode"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        if ctx.desktop_metadata.focus_mode {
            Ok(GuardOutcome::Deny {
                reason: "System is in focus/DND mode".into(),
            })
        } else {
            Ok(GuardOutcome::Allow)
        }
    }
}

/// Denies transition if AI is busy (inferencing, tool execution).
#[derive(Debug)]
pub struct AiNotBusy;

#[async_trait]
impl Guard for AiNotBusy {
    fn name(&self) -> &'static str {
        "AiNotBusy"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        if ctx.ai_metadata.is_inferencing || ctx.ai_metadata.is_tool_execution {
            Ok(GuardOutcome::Deny {
                reason: "AI is currently busy (inferencing or tool execution)".into(),
            })
        } else {
            Ok(GuardOutcome::Allow)
        }
    }
}

/// Denies transition if TTS is actively playing.
#[derive(Debug)]
pub struct VoiceNotSpeaking;

#[async_trait]
impl Guard for VoiceNotSpeaking {
    fn name(&self) -> &'static str {
        "VoiceNotSpeaking"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        if ctx.voice_metadata.is_speaking {
            Ok(GuardOutcome::Deny {
                reason: "Voice/TTS is currently speaking".into(),
            })
        } else {
            Ok(GuardOutcome::Allow)
        }
    }
}

/// Allows transition only if user has been idle for ≥ N seconds.
#[derive(Debug)]
pub struct IdleTimeExceeds {
    /// Minimum idle seconds required.
    pub min_seconds: u64,
}

#[async_trait]
impl Guard for IdleTimeExceeds {
    fn name(&self) -> &'static str {
        "IdleTimeExceeds"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        if ctx.desktop_metadata.idle_seconds >= self.min_seconds {
            Ok(GuardOutcome::Allow)
        } else {
            Ok(GuardOutcome::Deny {
                reason: format!(
                    "User has been idle for {}s, need {}s",
                    ctx.desktop_metadata.idle_seconds, self.min_seconds
                )
                .into(),
            })
        }
    }
}

/// Denies transition if an approval dialog is awaiting user input.
#[derive(Debug)]
pub struct NoActiveConfirmation;

#[async_trait]
impl Guard for NoActiveConfirmation {
    fn name(&self) -> &'static str {
        "NoActiveConfirmation"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        if ctx.workspace_metadata.has_active_confirmation {
            Ok(GuardOutcome::Deny {
                reason: "Awaiting user confirmation".into(),
            })
        } else {
            Ok(GuardOutcome::Allow)
        }
    }
}

/// A simple guard that always allows (useful as default).
#[derive(Debug)]
pub struct AllowAll;

#[async_trait]
impl Guard for AllowAll {
    fn name(&self) -> &'static str {
        "AllowAll"
    }

    async fn evaluate(
        &self,
        _ctx: &StateContext,
        _event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        Ok(GuardOutcome::Allow)
    }
}

/// Negates the result of another guard.
#[derive(Debug)]
pub struct NotGuard(pub Arc<dyn Guard>);

#[async_trait]
impl Guard for NotGuard {
    fn name(&self) -> &'static str {
        "Not"
    }

    async fn evaluate(
        &self,
        ctx: &StateContext,
        event: &StateEvent,
    ) -> Result<GuardOutcome, GuardError> {
        match self.0.evaluate(ctx, event).await? {
            GuardOutcome::Allow => Ok(GuardOutcome::Deny {
                reason: "Negated guard allowed".into(),
            }),
            GuardOutcome::Deny { .. } => Ok(GuardOutcome::Allow),
        }
    }
}
