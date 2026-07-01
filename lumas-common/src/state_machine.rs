//! # State Machine — Behavioral State Coordinator (Chapter 20)
//!
//! Defines the LumiState hierarchy, state transition rules, event triggers,
//! state commands consumed by downstream systems, and the core transition engine.

use crate::ai::AIState;
use crate::animation::{BlendMode, ClipId, EarPose};
use crate::character::CrystalState;
use crate::emotion::EmotionState;
use crate::position::PositionTarget;
use crate::workspace::PanelType;
use serde::{Deserialize, Serialize};

use crate::conversation::DetectedIntent;
use crate::plan::{PlanId, StepId};

// ---------------------------------------------------------------------------
// State Definitions
// ---------------------------------------------------------------------------

/// The complete Lumas state hierarchy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LumiState {
    /// Startup initialization.
    Initializing,
    /// Context-aware greeting at session start.
    Greeting { phase: GreetingPhase },
    /// Ambient idle presence with substates.
    Idle(IdleSubState),
    /// Do-not-disturb mode when user is focused.
    FocusMode,
    /// Sleeping after extended inactivity.
    Sleeping,
    /// Receiving input from voice or keyboard.
    Listening { source: InputSource },
    /// AI processing (thinking, inferencing).
    Processing { intent: Option<DetectedIntent> },
    /// Planning Engine is constructing a task plan.
    Planning { plan: PlanId, phase: PlanningPhase },
    /// AI Core is executing tool calls.
    Executing { plan: PlanId, step: StepId },
    /// Generating and streaming a response.
    Responding { response_type: ResponseType },
    /// Waiting for user confirmation before proceeding.
    AwaitingConfirmation { plan: PlanId, step: StepId },
    /// An error occurred, with recovery hint.
    Error {
        error: String,
        recovery: RecoveryHint,
    },
}

/// Phases within the greeting state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GreetingPhase {
    Starting,
    Speaking,
    Complete,
}

/// Substates for the idle behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdleSubState {
    /// Observing cursor and active window.
    Watching,
    /// Walking around desktop, inspecting windows.
    Exploring,
    /// Sitting, minimal animation.
    Resting,
    /// Lying down, slow breathing only.
    Sleeping,
}

/// Source of user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSource {
    Keyboard,
    Voice,
    Click,
    Drag,
}

/// Phases within the planning state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanningPhase {
    Analyzing,
    Generating,
    Reviewing,
}

/// Type of response being generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseType {
    Text,
    Voice,
    MultiModal,
}

/// Hint for recovery from errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryHint {
    /// Retry the last operation.
    Retry,
    /// Fall back to a different approach.
    Fallback,
    /// Ask the user for guidance.
    AskUser,
    /// Abort the current operation.
    Abort,
}

// ---------------------------------------------------------------------------
// Character State (for IPC signaling)
// ---------------------------------------------------------------------------

/// A simplified character state for IPC `ai.state` channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CharacterState {
    Idle,
    Active,
    Thinking,
    Speaking,
    Listening,
    Working,
    Sleeping,
    Error,
}

// ---------------------------------------------------------------------------
// State Events & Triggers
// ---------------------------------------------------------------------------

/// An event that may trigger a state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateEvent {
    /// New user input received.
    UserInput {
        source: InputSource,
        content: String,
    },
    /// Wake word detected.
    WakeWord { confidence: f32 },
    /// Voice speech ended.
    SpeechEnd { transcript: String },
    /// AI inference is complete.
    InferenceComplete,
    /// A plan was generated.
    PlanGenerated(PlanId),
    /// Plan was approved by user.
    PlanApproved(PlanId),
    /// Plan was cancelled by user.
    PlanCancelled(PlanId),
    /// Plan execution completed.
    PlanComplete(PlanId),
    /// Tool execution result received.
    ToolResult(StepId),
    /// Tool execution failed.
    ToolFailed(StepId, String),
    /// Confirmation requested from user.
    ConfirmationRequired(PlanId, StepId),
    /// User confirmed the operation.
    UserApproved(PlanId, StepId),
    /// User cancelled the operation.
    UserCancelled(PlanId, StepId),
    /// User became inactive.
    UserIdle { seconds: u64 },
    /// User became active again.
    UserActive,
    /// Focus/do-not-disturb detected.
    FocusDetected,
    /// Focus mode ended.
    FocusEnded,
    /// Response generation complete.
    ResponseComplete,
    /// An error occurred.
    Error(String),
    /// System startup complete.
    StartupComplete,
    /// Greeting animation complete.
    GreetingComplete,
    /// User clicked on Lumas.
    UserClick,
    /// User dragged Lumas.
    UserDrag { new_position: (f32, f32) },
    /// AI state changed.
    AIStateChanged(AIState),
}

/// Pattern for matching state machine transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StatePattern {
    /// Match a specific state exactly.
    Exact(LumiState),
    /// Match any state.
    Any,
    /// Match any idle substate.
    IdleAny,
    /// Match any state in a list.
    AnyOf(Vec<LumiState>),
    /// Match any state except the given ones.
    Not(Vec<LumiState>),
}

/// A trigger condition for a state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trigger {
    /// Match a specific event type.
    Event(StateEvent),
    /// Match any event.
    AnyEvent,
    /// Match events of a given AI state.
    AIState(AIState),
    /// Match after a timeout.
    Timeout { duration_ms: u64 },
}

// ---------------------------------------------------------------------------
// Transition Rules
// ---------------------------------------------------------------------------

/// Defines a single state transition: when to transition, where to go,
/// and what actions to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRule {
    pub from: StatePattern,
    pub trigger: Trigger,
    pub to: LumiState,
    /// Optional guard condition description.
    pub guard_description: Option<String>,
    /// Actions to execute upon transition.
    pub actions: Vec<StateAction>,
}

/// An action to execute during a state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateAction {
    /// Emit an AI state event.
    EmitAIState(AIState),
    /// Set the crystal state.
    SetCrystalState(CrystalState),
    /// Update workspace panel.
    UpdateWorkspace,
    /// Log the transition (for debugging).
    LogTransition,
}

// ---------------------------------------------------------------------------
// State Commands (Output)
// ---------------------------------------------------------------------------

/// Commands emitted by the State Machine to downstream systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateCommand {
    /// State has changed from one to another.
    StateChanged { from: LumiState, to: LumiState },
    /// Play an animation clip with the given blend mode.
    PlayAnimation { clip: ClipId, blend: BlendMode },
    /// Set the crystal state.
    SetCrystalState(CrystalState),
    /// Set the ear position target.
    SetEarTarget(EarPose),
    /// Set the emotion state.
    SetEmotionState(EmotionState),
    /// Move to a target position on the desktop.
    MoveTo(PositionTarget),
    /// Show a workspace panel.
    ShowWorkspacePanel(PanelType),
    /// Hide a workspace panel.
    HideWorkspacePanel(String),
    /// Enable or disable mouse passthrough on the window.
    EnableMousePassthrough(bool),
    /// Emit a particle effect.
    EmitParticleEffect(String),
    /// Play a sound effect.
    PlaySound(String),
}

// ---------------------------------------------------------------------------
// Default Transition Rules
// ---------------------------------------------------------------------------

/// Returns the standard set of state machine transition rules.
pub fn default_transition_rules() -> Vec<TransitionRule> {
    vec![
        // Initialization → Greeting
        TransitionRule {
            from: StatePattern::Exact(LumiState::Initializing),
            trigger: Trigger::Event(StateEvent::StartupComplete),
            to: LumiState::Greeting {
                phase: GreetingPhase::Starting,
            },
            guard_description: None,
            actions: vec![
                StateAction::EmitAIState(AIState::ReceivingInput),
                StateAction::LogTransition,
            ],
        },
        // Greeting → Idle
        TransitionRule {
            from: StatePattern::Exact(LumiState::Greeting {
                phase: GreetingPhase::Starting,
            }),
            trigger: Trigger::Event(StateEvent::GreetingComplete),
            to: LumiState::Idle(IdleSubState::Watching),
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Idle)],
        },
        // Idle → Listening
        TransitionRule {
            from: StatePattern::IdleAny,
            trigger: Trigger::Event(StateEvent::UserInput {
                source: InputSource::Keyboard,
                content: String::new(),
            }),
            to: LumiState::Listening {
                source: InputSource::Keyboard,
            },
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Listening)],
        },
        // Idle → Listening (wake word)
        TransitionRule {
            from: StatePattern::IdleAny,
            trigger: Trigger::Event(StateEvent::WakeWord { confidence: 0.0 }),
            to: LumiState::Listening {
                source: InputSource::Voice,
            },
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Listening)],
        },
        // Idle → Sleeping (after 15 min inactivity)
        TransitionRule {
            from: StatePattern::IdleAny,
            trigger: Trigger::Event(StateEvent::UserIdle { seconds: 900 }),
            to: LumiState::Sleeping,
            guard_description: None,
            actions: vec![StateAction::SetCrystalState(CrystalState {
                mode: crate::character::CrystalMode::Sleep,
                intensity: 0.1,
                color: crate::character::CrystalColor::WhiteSleep,
                pulse_rate: 0.5,
                particle_emit: false,
            })],
        },
        // Idle → FocusMode
        TransitionRule {
            from: StatePattern::IdleAny,
            trigger: Trigger::Event(StateEvent::FocusDetected),
            to: LumiState::FocusMode,
            guard_description: Some(String::from(
                "System do-not-disturb or fullscreen app active",
            )),
            actions: vec![StateAction::EmitAIState(AIState::Idle)],
        },
        // FocusMode → Idle
        TransitionRule {
            from: StatePattern::Exact(LumiState::FocusMode),
            trigger: Trigger::Event(StateEvent::FocusEnded),
            to: LumiState::Idle(IdleSubState::Watching),
            guard_description: None,
            actions: vec![],
        },
        // Listening → Processing (after speech end)
        TransitionRule {
            from: StatePattern::Exact(LumiState::Listening {
                source: InputSource::Voice,
            }),
            trigger: Trigger::Event(StateEvent::SpeechEnd {
                transcript: String::new(),
            }),
            to: LumiState::Processing { intent: None },
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Thinking)],
        },
        // Processing → Responding
        TransitionRule {
            from: StatePattern::Exact(LumiState::Processing { intent: None }),
            trigger: Trigger::Event(StateEvent::InferenceComplete),
            to: LumiState::Responding {
                response_type: ResponseType::Text,
            },
            guard_description: Some(String::from("No tools required")),
            actions: vec![StateAction::EmitAIState(AIState::GeneratingResponse)],
        },
        // Processing → Planning
        TransitionRule {
            from: StatePattern::Exact(LumiState::Processing { intent: None }),
            trigger: Trigger::Event(StateEvent::PlanGenerated(String::new())),
            to: LumiState::Planning {
                plan: String::new(),
                phase: PlanningPhase::Analyzing,
            },
            guard_description: Some(String::from("Plan was generated from request")),
            actions: vec![
                StateAction::EmitAIState(AIState::Planning),
                StateAction::UpdateWorkspace,
            ],
        },
        // Responding → Idle
        TransitionRule {
            from: StatePattern::Exact(LumiState::Responding {
                response_type: ResponseType::Text,
            }),
            trigger: Trigger::Event(StateEvent::ResponseComplete),
            to: LumiState::Idle(IdleSubState::Watching),
            guard_description: None,
            actions: vec![],
        },
        // Executing → Idle (plan complete)
        TransitionRule {
            from: StatePattern::Exact(LumiState::Executing {
                plan: String::new(),
                step: String::new(),
            }),
            trigger: Trigger::Event(StateEvent::PlanComplete(String::new())),
            to: LumiState::Idle(IdleSubState::Watching),
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Success)],
        },
        // Executing → AwaitingConfirmation
        TransitionRule {
            from: StatePattern::Exact(LumiState::Executing {
                plan: String::new(),
                step: String::new(),
            }),
            trigger: Trigger::Event(StateEvent::ConfirmationRequired(
                String::new(),
                String::new(),
            )),
            to: LumiState::AwaitingConfirmation {
                plan: String::new(),
                step: String::new(),
            },
            guard_description: Some(String::from("Tool requires user approval")),
            actions: vec![
                StateAction::EmitAIState(AIState::AwaitingConfirmation),
                StateAction::UpdateWorkspace,
            ],
        },
        // AwaitingConfirmation → Executing
        TransitionRule {
            from: StatePattern::Exact(LumiState::AwaitingConfirmation {
                plan: String::new(),
                step: String::new(),
            }),
            trigger: Trigger::Event(StateEvent::UserApproved(String::new(), String::new())),
            to: LumiState::Executing {
                plan: String::new(),
                step: String::new(),
            },
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::ExecutingTool)],
        },
        // AwaitingConfirmation → Idle (cancelled)
        TransitionRule {
            from: StatePattern::Exact(LumiState::AwaitingConfirmation {
                plan: String::new(),
                step: String::new(),
            }),
            trigger: Trigger::Event(StateEvent::UserCancelled(String::new(), String::new())),
            to: LumiState::Idle(IdleSubState::Watching),
            guard_description: None,
            actions: vec![],
        },
        // Any state → Error
        TransitionRule {
            from: StatePattern::Any,
            trigger: Trigger::Event(StateEvent::Error(String::new())),
            to: LumiState::Error {
                error: String::new(),
                recovery: RecoveryHint::AskUser,
            },
            guard_description: None,
            actions: vec![StateAction::EmitAIState(AIState::Error)],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_transition_rules_count() {
        let rules = default_transition_rules();
        assert!(!rules.is_empty());
        assert!(rules.len() >= 14);
    }

    #[test]
    fn test_transition_rule_structure() {
        let rules = default_transition_rules();
        for rule in &rules {
            // Every rule should have a target state
            match &rule.to {
                LumiState::Listening { .. }
                | LumiState::Processing { .. }
                | LumiState::Responding { .. }
                | LumiState::Planning { .. }
                | LumiState::Executing { .. }
                | LumiState::AwaitingConfirmation { .. }
                | LumiState::Error { .. }
                | LumiState::Idle(_)
                | LumiState::FocusMode
                | LumiState::Sleeping
                | LumiState::Initializing
                | LumiState::Greeting { .. } => {}
            }
        }
    }

    #[test]
    fn test_state_event_variants() {
        let events = vec![
            StateEvent::StartupComplete,
            StateEvent::GreetingComplete,
            StateEvent::WakeWord { confidence: 0.9 },
            StateEvent::UserActive,
            StateEvent::ResponseComplete,
            StateEvent::PlanApproved("plan-1".into()),
            StateEvent::PlanCancelled("plan-1".into()),
        ];
        for event in events {
            let json = serde_json::to_value(&event).unwrap();
            let back: StateEvent = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{event:?}"), format!("{back:?}"));
        }
    }

    #[test]
    fn test_lumas_state_serialization() {
        let states = vec![
            LumiState::Initializing,
            LumiState::Idle(IdleSubState::Watching),
            LumiState::Sleeping,
            LumiState::FocusMode,
            LumiState::Listening {
                source: InputSource::Keyboard,
            },
            LumiState::Error {
                error: "test".into(),
                recovery: RecoveryHint::Retry,
            },
        ];
        for state in states {
            let json = serde_json::to_value(&state).unwrap();
            let back: LumiState = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{state:?}"), format!("{back:?}"));
        }
    }

    #[test]
    fn test_state_command_variants() {
        let commands = vec![
            StateCommand::EnableMousePassthrough(true),
            StateCommand::PlaySound("success.wav".into()),
            StateCommand::EmitParticleEffect("sparkle".into()),
        ];
        for cmd in commands {
            let json = serde_json::to_value(&cmd).unwrap();
            let back: StateCommand = serde_json::from_value(json).unwrap();
            assert_eq!(format!("{cmd:?}"), format!("{back:?}"));
        }
    }
}
