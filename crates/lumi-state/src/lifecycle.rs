//! # Platform Lifecycle Machine
//!
//! The Runtime Lifecycle Machine is always the first machine registered.
//! Other machines may only register while the runtime is in `Running` or
//! `Initializing`. Attempting to register during `ShuttingDown` is an error.

use crate::action::{Action, RecordTransitionMetric};
use crate::error::{MachineId, StateId, StateResult};
use crate::event::events;
use crate::guard::{AllowAll, Guard};
use crate::machine::StateMachine;
use crate::state::{LeafState, MachineState, StateContext, StateTimeout};
use crate::transition::TransitionDefinition;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Runtime Lifecycle States
// ---------------------------------------------------------------------------

/// Initializing state.
#[derive(Debug)]
pub struct InitializingState;

#[async_trait]
impl MachineState for InitializingState {
    fn id(&self) -> StateId {
        StateId(5000)
    }
    fn name(&self) -> &'static str {
        "Initializing"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
}

/// Running state (normal operation).
#[derive(Debug)]
pub struct RunningState {
    started_at: std::time::Instant,
}

impl RunningState {
    pub fn new() -> Self {
        Self {
            started_at: std::time::Instant::now(),
        }
    }
}

#[async_trait]
impl MachineState for RunningState {
    fn id(&self) -> StateId {
        StateId(5001)
    }
    fn name(&self) -> &'static str {
        "Running"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
}

/// Updating state (applying an update).
#[derive(Debug)]
pub struct UpdatingState;

#[async_trait]
impl MachineState for UpdatingState {
    fn id(&self) -> StateId {
        StateId(5002)
    }
    fn name(&self) -> &'static str {
        "Updating"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
}

/// Restarting state (restarting after update).
#[derive(Debug)]
pub struct RestartingState;

#[async_trait]
impl MachineState for RestartingState {
    fn id(&self) -> StateId {
        StateId(5003)
    }
    fn name(&self) -> &'static str {
        "Restarting"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
}

/// ShuttingDown state (graceful shutdown in progress).
#[derive(Debug)]
pub struct ShuttingDownState;

#[async_trait]
impl MachineState for ShuttingDownState {
    fn id(&self) -> StateId {
        StateId(5004)
    }
    fn name(&self) -> &'static str {
        "ShuttingDown"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
    fn is_final(&self) -> bool {
        false
    }
}

/// Stopped state (terminal).
#[derive(Debug)]
pub struct StoppedState;

#[async_trait]
impl MachineState for StoppedState {
    fn id(&self) -> StateId {
        StateId(5005)
    }
    fn name(&self) -> &'static str {
        "Stopped"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
    fn is_final(&self) -> bool {
        true
    }
}

/// Failed state (terminal, unrecoverable).
#[derive(Debug)]
pub struct FailedState;

#[async_trait]
impl MachineState for FailedState {
    fn id(&self) -> StateId {
        StateId(5006)
    }
    fn name(&self) -> &'static str {
        "Failed"
    }
    fn machine_id(&self) -> MachineId {
        MachineId::RUNTIME
    }
    fn is_final(&self) -> bool {
        true
    }
}

// =========================================================================
// Build Runtime Machine
// =========================================================================

/// Build the complete Runtime Lifecycle Machine.
///
/// States:
/// Initializing → (startup_complete) → Running
/// Running      → (shutdown_requested) → ShuttingDown
/// Running      → (update_available) → Updating
/// Running      → (fatal_error) → Failed
/// Updating     → (update_complete) → Restarting
/// Updating     → (update_failed) → Running
/// Restarting   → (restart_complete) → Initializing
/// ShuttingDown → (shutdown_complete) → Stopped
/// Failed       → (recovery_triggered) → Initializing
pub fn build_runtime_machine() -> StateMachine {
    let initializing = Arc::new(InitializingState);
    let running = Arc::new(RunningState::new());
    let updating = Arc::new(UpdatingState);
    let restarting = Arc::new(RestartingState);
    let shutting_down = Arc::new(ShuttingDownState);
    let stopped = Arc::new(StoppedState);
    let failed = Arc::new(FailedState);

    let mut machine = StateMachine::new(MachineId::RUNTIME, "RuntimeLifecycle");
    machine.initial_state = StateId(5000);

    // Add states
    machine.add_state(initializing);
    machine.add_state(running);
    machine.add_state(updating);
    machine.add_state(restarting);
    machine.add_state(shutting_down);
    machine.add_state(stopped);
    machine.add_state(failed);

    // Final states
    machine.final_states.insert(StateId(5005)); // Stopped
    machine.final_states.insert(StateId(5006)); // Failed

    // Transitions
    let allow = Arc::new(AllowAll);

    // Initializing → Running
    machine.add_transition(
        TransitionDefinition::new(
            1u32,
            StateId(5000),
            StateId(5001),
            events::RUNTIME_STARTUP_COMPLETE,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Running → ShuttingDown
    machine.add_transition(
        TransitionDefinition::new(
            2u32,
            StateId(5001),
            StateId(5004),
            events::RUNTIME_SHUTDOWN_REQUESTED,
        )
        .with_guard(allow.clone())
        .with_exit_action(Arc::new(LogTransition))
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Running → Updating
    machine.add_transition(
        TransitionDefinition::new(
            3u32,
            StateId(5001),
            StateId(5002),
            events::RUNTIME_UPDATE_AVAILABLE,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Running → Failed
    machine.add_transition(
        TransitionDefinition::new(
            4u32,
            StateId(5001),
            StateId(5006),
            events::RUNTIME_FATAL_ERROR,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Updating → Restarting
    machine.add_transition(
        TransitionDefinition::new(
            5u32,
            StateId(5002),
            StateId(5003),
            events::RUNTIME_RESTART_COMPLETE,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Updating → Running (update failed)
    machine.add_transition(
        TransitionDefinition::new(
            6u32,
            StateId(5002),
            StateId(5001),
            events::RUNTIME_UPDATE_FAILED,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Restarting → Initializing
    machine.add_transition(
        TransitionDefinition::new(
            7u32,
            StateId(5003),
            StateId(5000),
            events::RUNTIME_RESTART_STARTED,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // ShuttingDown → Stopped
    machine.add_transition(
        TransitionDefinition::new(
            8u32,
            StateId(5004),
            StateId(5005),
            events::RUNTIME_POWER_SLEEP,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    // Failed → Initializing (recovery)
    machine.add_transition(
        TransitionDefinition::new(
            9u32,
            StateId(5006),
            StateId(5000),
            events::RUNTIME_RECOVERY_TRIGGERED,
        )
        .with_guard(allow.clone())
        .with_entry_action(Arc::new(RecordTransitionMetric)),
    );

    machine
}

// =========================================================================
// Character Machine
// =========================================================================

/// Build the complete Character Behavior Machine.
///
/// States:
/// Idle (composite: Watching, Exploring, Resting)
/// Interacting (composite: Listening, Thinking, Speaking, AwaitingInput)
/// Working (composite: Preparing, Executing, VerifyingResult — shallow history H)
/// Sleeping
/// FocusMode
pub fn build_character_machine() -> StateMachine {
    use crate::hierarchy::HistoryKind;

    let watching = Arc::new(LeafState::new(
        StateId(1100),
        "Watching",
        MachineId::CHARACTER,
    ));
    let exploring = Arc::new(LeafState::new(
        StateId(1101),
        "Exploring",
        MachineId::CHARACTER,
    ));
    let resting = Arc::new(LeafState::new(
        StateId(1102),
        "Resting",
        MachineId::CHARACTER,
    ));
    let listening = Arc::new(LeafState::new(
        StateId(1200),
        "Listening",
        MachineId::CHARACTER,
    ));
    let thinking = Arc::new(LeafState::new(
        StateId(1201),
        "Thinking",
        MachineId::CHARACTER,
    ));
    let speaking = Arc::new(LeafState::new(
        StateId(1202),
        "Speaking",
        MachineId::CHARACTER,
    ));
    let awaiting_input = Arc::new(LeafState::new(
        StateId(1203),
        "AwaitingInput",
        MachineId::CHARACTER,
    ));
    let preparing = Arc::new(LeafState::new(
        StateId(1300),
        "Preparing",
        MachineId::CHARACTER,
    ));
    let executing = Arc::new(LeafState::new(
        StateId(1301),
        "Executing",
        MachineId::CHARACTER,
    ));
    let verifying_result = Arc::new(LeafState::new(
        StateId(1302),
        "VerifyingResult",
        MachineId::CHARACTER,
    ));
    let sleeping = Arc::new(LeafState::new(
        StateId(1400),
        "Sleeping",
        MachineId::CHARACTER,
    ));
    let focus_mode = Arc::new(LeafState::new(
        StateId(1500),
        "FocusMode",
        MachineId::CHARACTER,
    ));

    let mut machine = StateMachine::new(MachineId::CHARACTER, "CharacterBehavior");
    machine.initial_state = StateId(1100); // Watching

    machine.add_state(watching);
    machine.add_state(exploring);
    machine.add_state(resting);
    machine.add_state(listening);
    machine.add_state(thinking);
    machine.add_state(speaking);
    machine.add_state(awaiting_input);
    machine.add_state(preparing);
    machine.add_state(executing);
    machine.add_state(verifying_result);
    machine.add_state(sleeping);
    machine.add_state(focus_mode);

    // Transitions
    let allow = Arc::new(AllowAll);

    machine.add_transition(
        TransitionDefinition::new(
            100u32,
            StateId(1100),
            StateId(1200),
            events::CHAR_CURSOR_MOVED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            101u32,
            StateId(1100),
            StateId(1400),
            events::CHAR_SLEEP_TIMER_EXPIRED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            102u32,
            StateId(1100),
            StateId(1101),
            events::CHAR_EXPLORING_TIMER,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            103u32,
            StateId(1100),
            StateId(1102),
            events::CHAR_RESTING_TIMER,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            104u32,
            StateId(1400),
            StateId(1100),
            events::CHAR_USER_ACTIVE,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            105u32,
            StateId(1100),
            StateId(1500),
            events::CHAR_FOCUS_MODE_ENTERED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            106u32,
            StateId(1500),
            StateId(1100),
            events::CHAR_FOCUS_MODE_EXITED,
        )
        .with_guard(allow.clone()),
    );

    machine
}

/// Build the complete AI Processing Machine.
pub fn build_ai_machine() -> StateMachine {
    let ready = Arc::new(LeafState::new(StateId(2100), "Ready", MachineId::AI));
    let building_context = Arc::new(LeafState::new(
        StateId(2101),
        "BuildingContext",
        MachineId::AI,
    ));
    let retrieving_memory = Arc::new(LeafState::new(
        StateId(2102),
        "RetrievingMemory",
        MachineId::AI,
    ));
    let waiting_first_token = Arc::new(LeafState::new(
        StateId(2103),
        "WaitingFirstToken",
        MachineId::AI,
    ));
    let streaming = Arc::new(LeafState::new(StateId(2104), "Streaming", MachineId::AI));
    let reflecting = Arc::new(LeafState::new(StateId(2105), "Reflecting", MachineId::AI));
    let generating_plan = Arc::new(LeafState::new(
        StateId(2106),
        "GeneratingPlan",
        MachineId::AI,
    ));
    let awaiting_approval = Arc::new(LeafState::new(
        StateId(2107),
        "AwaitingApproval",
        MachineId::AI,
    ));
    let starting_tool = Arc::new(LeafState::new(StateId(2108), "StartingTool", MachineId::AI));
    let running_tool = Arc::new(LeafState::new(StateId(2109), "RunningTool", MachineId::AI));
    let collecting_result = Arc::new(LeafState::new(
        StateId(2110),
        "CollectingResult",
        MachineId::AI,
    ));
    let cancelled = Arc::new(LeafState::new(StateId(2111), "Cancelled", MachineId::AI));
    let failed = Arc::new(LeafState::new(StateId(2112), "Failed", MachineId::AI));

    let mut machine = StateMachine::new(MachineId::AI, "AIProcessing");
    machine.initial_state = StateId(2100); // Ready

    machine.add_state(ready);
    machine.add_state(building_context);
    machine.add_state(retrieving_memory);
    machine.add_state(waiting_first_token);
    machine.add_state(streaming);
    machine.add_state(reflecting);
    machine.add_state(generating_plan);
    machine.add_state(awaiting_approval);
    machine.add_state(starting_tool);
    machine.add_state(running_tool);
    machine.add_state(collecting_result);
    machine.add_state(cancelled);
    machine.add_state(failed);

    machine.final_states.insert(StateId(2111)); // Cancelled
    machine.final_states.insert(StateId(2112)); // Failed

    let allow = Arc::new(AllowAll);

    // Ready → Processing
    machine.add_transition(
        TransitionDefinition::new(
            200u32,
            StateId(2100),
            StateId(2101),
            events::AI_INPUT_RECEIVED,
        )
        .with_guard(allow.clone()),
    );

    // Processing transitions
    machine.add_transition(
        TransitionDefinition::new(
            201u32,
            StateId(2101),
            StateId(2102),
            events::AI_MEMORY_RETRIEVED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            202u32,
            StateId(2102),
            StateId(2103),
            events::AI_INFERENCE_STARTED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            203u32,
            StateId(2103),
            StateId(2104),
            events::AI_FIRST_TOKEN_RECEIVED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            204u32,
            StateId(2104),
            StateId(2106),
            events::AI_TOOL_CALL_REQUESTED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            205u32,
            StateId(2104),
            StateId(2100),
            events::AI_RESPONSE_COMPLETE,
        )
        .with_guard(allow.clone()),
    );

    // Tool execution
    machine.add_transition(
        TransitionDefinition::new(
            206u32,
            StateId(2106),
            StateId(2109),
            events::AI_TOOL_CALL_REQUESTED,
        )
        .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(
            207u32,
            StateId(2109),
            StateId(2100),
            events::AI_TOOL_CALL_COMPLETE,
        )
        .with_guard(allow.clone()),
    );

    // Error / Cancel paths
    machine.add_transition(
        TransitionDefinition::new(208u32, StateId(2101), StateId(2111), events::AI_CANCELLED)
            .with_guard(allow.clone()),
    );
    machine.add_transition(
        TransitionDefinition::new(209u32, StateId(2104), StateId(2112), events::AI_ERROR)
            .with_guard(allow.clone()),
    );

    machine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_machine_validation() {
        let machine = build_runtime_machine();
        assert!(machine.validate().is_ok());
    }

    #[test]
    fn test_character_machine_validation() {
        let machine = build_character_machine();
        assert!(machine.validate().is_ok());
    }

    #[test]
    fn test_ai_machine_validation() {
        let machine = build_ai_machine();
        assert!(machine.validate().is_ok());
    }

    #[test]
    fn test_runtime_machine_has_all_states() {
        let machine = build_runtime_machine();
        assert!(machine.has_state(StateId(5000))); // Initializing
        assert!(machine.has_state(StateId(5001))); // Running
        assert!(machine.has_state(StateId(5004))); // ShuttingDown
        assert!(machine.has_state(StateId(5005))); // Stopped
    }

    #[test]
    fn test_ai_machine_final_states() {
        let machine = build_ai_machine();
        assert!(machine.is_final(StateId(2111))); // Cancelled
        assert!(machine.is_final(StateId(2112))); // Failed
    }
}
