//! # Action System
//!
//! Actions are side effects executed during a state transition.
//! They can be blocking (transition waits) or detached (fire-and-forget).

use crate::context::StateContext;
use crate::error::StateResult;
use crate::event::StateEvent;
use async_trait::async_trait;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// An action executed during a state transition.
///
/// Actions are called at three points:
/// - Exit actions: when leaving the current state
/// - Transition actions: during the transition itself
/// - Entry actions: when entering the new state
///
/// # Errors
/// Action failures are handled according to the transition step:
/// - Exit action failure → rollback
/// - Transition action failure → rollback
/// - Entry action failure → error state (already committed)
#[async_trait]
pub trait Action: Send + Sync + fmt::Debug + 'static {
    /// Human-readable action name.
    fn name(&self) -> &'static str;

    /// Whether this action blocks the transition or is fire-and-forget.
    fn execution_mode(&self) -> ActionMode {
        ActionMode::Blocking
    }

    /// Execute the action.
    async fn execute(&self, ctx: &mut StateContext, event: &StateEvent) -> StateResult<()>;
}

/// Execution mode for actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionMode {
    /// Transition waits for this action to complete.
    Blocking,
    /// Action is spawned; transition continues immediately.
    Detached,
    /// Detached with a timeout — logged if exceeds.
    DetachedWithTimeout(std::time::Duration),
}

/// A no-op action that does nothing immediately.
#[derive(Debug)]
pub struct NoOp;

#[async_trait]
impl Action for NoOp {
    fn name(&self) -> &'static str {
        "NoOp"
    }

    async fn execute(&self, _ctx: &mut StateContext, _event: &StateEvent) -> StateResult<()> {
        Ok(())
    }
}

/// Logs a transition event.
#[derive(Debug)]
pub struct LogTransition;

#[async_trait]
impl Action for LogTransition {
    fn name(&self) -> &'static str {
        "LogTransition"
    }

    async fn execute(&self, ctx: &mut StateContext, event: &StateEvent) -> StateResult<()> {
        tracing::info!(
            "State transition on machine {:?}: {:?} triggered by {}",
            ctx.machine_id,
            ctx.current_state,
            event.name
        );
        Ok(())
    }
}

/// Records a transition metric.
#[derive(Debug)]
pub struct RecordTransitionMetric;

#[async_trait]
impl Action for RecordTransitionMetric {
    fn name(&self) -> &'static str {
        "RecordTransitionMetric"
    }

    async fn execute(&self, ctx: &mut StateContext, _event: &StateEvent) -> StateResult<()> {
        ctx.transition_count += 1;
        tracing::trace!(
            "Transition #{} recorded for machine {:?}",
            ctx.transition_count,
            ctx.machine_id
        );
        Ok(())
    }
}

/// Reports an error to the error system.
#[derive(Debug)]
pub struct NotifyErrorSystem {
    /// Error message to report.
    pub message: String,
}

#[async_trait]
impl Action for NotifyErrorSystem {
    fn name(&self) -> &'static str {
        "NotifyErrorSystem"
    }

    async fn execute(&self, _ctx: &mut StateContext, _event: &StateEvent) -> StateResult<()> {
        tracing::error!("State machine error: {}", self.message);
        Ok(())
    }
}
