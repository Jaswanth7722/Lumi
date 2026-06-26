//! # Transition Engine
//!
//! The core transition execution engine that enforces the six-step atomicity
//! protocol defined in the architecture constraints.
//!
//! Steps 1–3 are tentative: any failure aborts and the machine stays in the source state.
//! Step 4 is the commit point: an atomic operation ensures readers see a valid state.
//! Steps 5–6 occur after commit; failures here produce errors but do not roll back.

use crate::action::Action;
use crate::context::StateContext;
use crate::error::{EventId, GuardError, MachineId, StateId, StateResult, TransitionId};
use crate::event::StateEvent;
use crate::guard::Guard;
use crate::state::MachineState;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A single transition definition in a transition table.
#[derive(Debug, Clone)]
pub struct TransitionDefinition {
    /// Transition ID (stable across releases).
    pub id: TransitionId,
    /// Source state ID.
    pub source: StateId,
    /// Target state ID.
    pub target: StateId,
    /// Trigger event ID.
    pub trigger: EventId,
    /// Guards that must pass for this transition to fire.
    pub guards: Vec<Arc<dyn Guard>>,
    /// Actions to execute when exiting the source state.
    pub exit_actions: Vec<Arc<dyn Action>>,
    /// Actions to execute during the transition (between exit and entry).
    pub transition_actions: Vec<Arc<dyn Action>>,
    /// Actions to execute when entering the target state.
    pub entry_actions: Vec<Arc<dyn Action>>,
    /// Priority: higher value = evaluated first when multiple transitions match same event.
    pub priority: i32,
    /// If true, this is an internal transition: no exit/entry actions, state unchanged.
    pub is_internal: bool,
    /// If true, exit action failure causes a full rollback.
    pub rollback_on_exit_failure: bool,
}

impl TransitionDefinition {
    /// Create a new transition definition.
    pub fn new(
        id: impl Into<TransitionId>,
        source: StateId,
        target: StateId,
        trigger: EventId,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            target,
            trigger,
            guards: Vec::new(),
            exit_actions: Vec::new(),
            transition_actions: Vec::new(),
            entry_actions: Vec::new(),
            priority: 0,
            is_internal: false,
            rollback_on_exit_failure: true,
        }
    }

    /// Add a guard.
    pub fn with_guard(mut self, guard: Arc<dyn Guard>) -> Self {
        self.guards.push(guard);
        self
    }

    /// Add an exit action.
    pub fn with_exit_action(mut self, action: Arc<dyn Action>) -> Self {
        self.exit_actions.push(action);
        self
    }

    /// Add a transition action.
    pub fn with_transition_action(mut self, action: Arc<dyn Action>) -> Self {
        self.transition_actions.push(action);
        self
    }

    /// Add an entry action.
    pub fn with_entry_action(mut self, action: Arc<dyn Action>) -> Self {
        self.entry_actions.push(action);
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Mark as internal transition.
    pub fn internal(mut self) -> Self {
        self.is_internal = true;
        self
    }
}

/// The step in the transition protocol where a failure occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStep {
    /// Guard evaluation phase.
    GuardEvaluation,
    /// Exit action phase.
    ExitAction,
    /// Transition action phase.
    TransitionAction,
    /// State commit phase (should never fail).
    StateCommit,
    /// Entry action phase (post-commit).
    EntryAction,
    /// Event publication phase (post-commit, best-effort).
    EventPublication,
}

/// The outcome of a transition execution.
#[derive(Debug, Clone)]
pub enum TransitionOutcome {
    /// Transition completed successfully.
    Completed {
        from: StateId,
        to: StateId,
        duration: Duration,
        /// Entry action errors that occurred after commit (non-fatal).
        entry_action_errors: Vec<ActionError>,
    },
    /// Transition was rejected by guards.
    Rejected {
        reason: GuardRejection,
        evaluated_guards: Vec<GuardResult>,
    },
    /// Transition was rolled back due to a failure in steps 1–3.
    RolledBack {
        at_step: TransitionStep,
        cause: String,
    },
}

/// A guard evaluation result.
#[derive(Debug, Clone)]
pub struct GuardResult {
    /// Guard name.
    pub name: &'static str,
    /// Whether the guard allowed or denied.
    pub outcome: crate::guard::GuardOutcome,
}

/// Why a transition was rejected by guards.
#[derive(Debug, Clone)]
pub enum GuardRejection {
    /// A guard returned Deny.
    Denied {
        guard_name: &'static str,
        reason: String,
    },
    /// No matching transition found for this event in the current state.
    NoMatchingTransition { source: StateId, event: EventId },
}

/// An action that failed during transition execution.
#[derive(Debug, Clone)]
pub struct ActionError {
    /// Action name.
    pub action_name: &'static str,
    /// Error message.
    pub message: String,
    /// When in the transition this action was executed.
    pub step: TransitionStep,
}

// =========================================================================
// Transition Engine
// =========================================================================

/// Engine that executes transitions following the atomicity protocol.
#[derive(Debug)]
pub struct TransitionEngine {
    /// Default action timeout.
    action_timeout: Duration,
    /// Default guard timeout.
    guard_timeout: Duration,
}

impl TransitionEngine {
    /// Create a new transition engine.
    pub fn new(action_timeout: Duration, guard_timeout: Duration) -> Self {
        Self {
            action_timeout,
            guard_timeout,
        }
    }

    /// Execute a transition following the six-step atomicity protocol.
    ///
    /// # Protocol
    ///
    /// 1. **Guard Evaluation** — all guards evaluated; any failure → reject
    /// 2. **Exit Actions** — current state's exit actions; failure → rollback
    /// 3. **Transition Actions** — transition-level actions; failure → rollback
    /// 4. **State Commit** — atomic state update (COMMIT POINT — cannot fail)
    /// 5. **Entry Actions** — new state's entry actions; failure → error state
    /// 6. **Event Publication** — publish transition event (best-effort)
    pub async fn execute(
        &self,
        source_state: &Arc<dyn MachineState>,
        target_state: &Arc<dyn MachineState>,
        transition: &TransitionDefinition,
        event: &StateEvent,
        ctx: &mut StateContext,
    ) -> TransitionOutcome {
        if transition.is_internal {
            return TransitionOutcome::Completed {
                from: transition.source,
                to: transition.source,
                duration: Duration::ZERO,
                entry_action_errors: Vec::new(),
            };
        }

        let start = Instant::now();

        // STEP 1: Guard Evaluation
        let mut guard_results = Vec::new();
        for guard in &transition.guards {
            let outcome =
                tokio::time::timeout(self.guard_timeout, guard.evaluate(ctx, event)).await;

            match outcome {
                Ok(Ok(guard_result)) => {
                    let result = GuardResult {
                        name: guard.name(),
                        outcome: guard_result.clone(),
                    };
                    guard_results.push(result);

                    match guard_result {
                        crate::guard::GuardOutcome::Deny { reason } => {
                            return TransitionOutcome::Rejected {
                                reason: GuardRejection::Denied {
                                    guard_name: guard.name(),
                                    reason: reason.to_string(),
                                },
                                evaluated_guards: guard_results,
                            };
                        }
                        crate::guard::GuardOutcome::Allow => {}
                    }
                }
                Ok(Err(e)) => {
                    return TransitionOutcome::Rejected {
                        reason: GuardRejection::Denied {
                            guard_name: guard.name(),
                            reason: format!("Guard error: {}", e.message),
                        },
                        evaluated_guards: guard_results,
                    };
                }
                Err(_) => {
                    return TransitionOutcome::Rejected {
                        reason: GuardRejection::Denied {
                            guard_name: guard.name(),
                            reason: "Guard evaluation timed out".into(),
                        },
                        evaluated_guards: guard_results,
                    };
                }
            }
        }

        // STEP 2: Exit Actions (tentative — can roll back)
        // First, call the source state's lifecycle on_exit
        if let Err(e) = source_state.on_exit(ctx).await {
            if transition.rollback_on_exit_failure {
                return TransitionOutcome::RolledBack {
                    at_step: TransitionStep::ExitAction,
                    cause: format!("Source state on_exit failed: {}", e),
                };
            }
        }

        for action in &transition.exit_actions {
            if let Err(e) = self.execute_action(action, ctx, event).await {
                if transition.rollback_on_exit_failure {
                    return TransitionOutcome::RolledBack {
                        at_step: TransitionStep::ExitAction,
                        cause: format!("Exit action '{}' failed: {}", action.name(), e),
                    };
                }
            }
        }

        // STEP 3: Transition Actions (tentative — can roll back)
        for action in &transition.transition_actions {
            if let Err(e) = self.execute_action(action, ctx, event).await {
                return TransitionOutcome::RolledBack {
                    at_step: TransitionStep::TransitionAction,
                    cause: format!("Transition action '{}' failed: {}", action.name(), e),
                };
            }
        }

        // STEP 4: State Commit (COMMIT POINT — from here, no rollback)
        let from = ctx.current_state;
        ctx.previous_state = Some(from);
        ctx.current_state = transition.target;
        ctx.state_entered_at = Instant::now();

        // STEP 5: Entry Actions (post-commit — failures produce errors but no rollback)
        let mut entry_action_errors = Vec::new();

        // First, call the target state's lifecycle on_entry
        if let Err(e) = target_state.on_entry(ctx).await {
            entry_action_errors.push(ActionError {
                action_name: target_state.name(),
                message: format!("on_entry failed: {}", e),
                step: TransitionStep::EntryAction,
            });
        }

        // Then execute transition-defined entry actions
        for action in &transition.entry_actions {
            if let Err(e) = self.execute_action(action, ctx, event).await {
                entry_action_errors.push(ActionError {
                    action_name: action.name(),
                    message: e.to_string(),
                    step: TransitionStep::EntryAction,
                });
            }
        }

        // STEP 6: Event Publication (best-effort)
        let duration = start.elapsed();

        TransitionOutcome::Completed {
            from,
            to: transition.target,
            duration,
            entry_action_errors,
        }
    }

    /// Execute a single action with timeout.
    async fn execute_action(
        &self,
        action: &Arc<dyn Action>,
        ctx: &mut StateContext,
        event: &StateEvent,
    ) -> StateResult<()> {
        match action.execution_mode() {
            crate::action::ActionMode::Blocking => {
                tokio::time::timeout(self.action_timeout, action.execute(ctx, event))
                    .await
                    .map_err(|_| crate::error::StateError::ActionTimeout {
                        action_name: action.name(),
                        elapsed: self.action_timeout,
                    })?
            }
            crate::action::ActionMode::Detached => {
                // Fire-and-forget
                let _ = action.execute(ctx, event).await;
                Ok(())
            }
            crate::action::ActionMode::DetachedWithTimeout(timeout) => {
                if tokio::time::timeout(timeout, action.execute(ctx, event))
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        "Detached action '{}' exceeded timeout {:?}",
                        action.name(),
                        timeout
                    );
                }
                Ok(())
            }
        }
    }
}
